// rustyVM - Java VM written in pure Rust
// Copyright (c) 2013 Alexander Gessler
//
// Permission is hereby granted, free of charge, to any person obtaining a copy of
// this software and associated documentation files (the "Software"), to deal in
// the Software without restriction, including without limitation the rights to
// use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of
// the Software, and to permit persons to whom the Software is furnished to do so,
// subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS
// FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR
// COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER
// IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN
// CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
//


use std::hashmap::{HashMap};
use std::path::{PosixPath};
use std::io::{result, IoError, BufReader};
use std::str::{from_utf8_owned};

use extra::future::{Future};
use extra::arc::{Arc, MutexArc};

use def::*;
use class::{JavaClass, JavaClassRef, JavaClassFutureRef};
use classpath::{ClassPath};
use code::{CodeBlock, ExceptionHandler};
use method::{JavaMethod};


// Abstract trait to describe a class loader's basic behaviour
pub trait AbstractClassLoader {

	// ----------------------------------------------
	// Asynchronously loads a class with the given name using
	// an implementation-defined method to locate the class file. In 
	// case of failure, a simple string error message is returned.
	//
	// See ClassLoader::add_from_classfile() for the default impl.
	fn load(&mut self, name : &str) -> JavaClassFutureRef;


	// ----------------------------------------------
	// Asynchronously loads a class with the given name from
	// the provided byte buffer.
	//
	// See ClassLoader::add_from_bytes() for the default impl.
	fn load_from_bytes(&mut self, name : &str, bytes : ~[u8]) -> JavaClassFutureRef;
}


// ClassLoader is clonable as to enable every task to have a copy of it.
// However, all ClassLoader's derived from the same original loader share
// their internal state through a concurrent hash map.
pub struct ClassLoader {
	priv classpath : ClassPath,
	priv ClassTableRef : ClassTableRef
}


impl AbstractClassLoader for ClassLoader {

	fn load(&mut self, name : &str) -> JavaClassFutureRef	{
		return self.add_from_classfile(name);
	}

	fn load_from_bytes(&mut self, name : &str, bytes : ~[u8]) -> JavaClassFutureRef	{
		return self.add_from_bytes(name, bytes);
	}
}



enum JavaClassOrWaitQueue {
	ClassLoaded(JavaClassRef),
	ClassPending(~[Chan<Result<JavaClassRef, ~str>>]),
}

// table of java classes indexed by fully qualified name
type ClassTable = HashMap<~str, JavaClassOrWaitQueue>;
type ClassTableRef = MutexArc<ClassTable>;


static INITIAL_CLASSLOADER_CAPACITY : uint = 1024;

impl ClassLoader {

	// ----------------------------------------------
	// Constructs a new classloader given a semicolon-separated list
	// of viable paths to search for .class files.
	pub fn new_from_string(classpath : &str) -> ClassLoader {
		ClassLoader::new(
				ClassPath::new_from_string(classpath),
				MutexArc::new(HashMap::with_capacity(INITIAL_CLASSLOADER_CAPACITY))
		)
	}


	// ----------------------------------------------
	// Construction using an explicit ClassPath instance, and a shared
	// class table. This is used to have multiple ClassLoader instances
	// share their internal concurrent state.
	pub fn new(classpath : ClassPath, ClassTableRef : ClassTableRef) -> ClassLoader {
		ClassLoader {
			classpath : classpath,
			ClassTableRef : ClassTableRef,
		}
	}


	// ----------------------------------------------
	// Get the immutable classpath that backs this classloader
	pub fn get_classpath(&self) -> ClassPath
	{
		return self.classpath.clone();
	}


	// ----------------------------------------------
	// Get a class if it has been loaded already. Does not attempt to
	// load a class, or wait for loading to be completed. This method is an
	// inherent race condition as more classes may be added concurrently.
	pub fn get_class(&self, name : &str) -> Option<JavaClassRef>
	{
		let cname = name.into_owned();
		unsafe { 
			self.ClassTableRef.unsafe_access(|table : &mut ClassTable| {
				match table.find(&cname) {
					Some(&ClassLoaded(ref elem)) => Some((*elem).clone()),
					_ => None
				}
			})
		}
	}

	
	// ----------------------------------------------
	// Load a class given its fully name. The .class file for the class is
	// located in the classpath and loaded. Afterwards, the java class is
	// prepared for use with the VM and ultimatively returned. Loading is
	// asynchronous.
	pub fn add_from_classfile(&mut self, name : &str) -> JavaClassFutureRef {
		// do nothing if the class is already loaded,
		// if it is already being loaded, add ourselves to the list of waiters
		let cname = name.into_owned();

		let res = self.check_is_present_or_enqueue(name);
		if res.is_some() {
			return res.unwrap();
		}

		debug!("start async loading of class {} from a classpath location", name);

		// TODO: inform waiters also if loading fails

		let self_clone_outer = self.clone();
		let fut = do Future::spawn {
			// TODO: if we don't clone() twice, borrowch complains.
			// May be resolved through https://github.com/mozilla/rust/issues/10617
			let mut self_clone = self_clone_outer.clone();
			match self_clone.classpath.locate_and_read(cname) {
				None => Err(~"failed to locate class file for " + cname),
				Some(bytes) => {
					self_clone.intern_add_from_classfile_bytes(cname, bytes)
				}
			}
		};
		JavaClassFutureRef::new(fut)
	}


	// ----------------------------------------------
	// Load a class given a byte stream containing a valid .class file.
	// The given name is the fully qualified name name under which the class 
	// is added to the class hierarchy.
	pub fn add_from_bytes(&mut self, name : &str, bytes : ~[u8]) -> JavaClassFutureRef {
		let res = self.check_is_present_or_enqueue(name);
		if res.is_some() {
			return res.unwrap();
		}

		debug!("start async loading of class {} from a memory location", name);

		// TODO: inform waiters also if loading fails

		let self_clone_outer = self.clone();
		let cname = name.into_owned();

		let fut = do Future::spawn {
			// TODO: if we don't clone() twice, borrowch complains.
			// May be resolved through https://github.com/mozilla/rust/issues/10617
			let mut self_clone = self_clone_outer.clone();
			self_clone.intern_add_from_classfile_bytes(cname, bytes)
		};
		JavaClassFutureRef::new(fut)
	}


	// IMPL


	// ----------------------------------------------
	// Check if a class with the given name is pending loading or
	// has been loaded already. In the first case a waiter is enqueued
	// to receive the result of the pending loading and in the latter 
	// case the future is constructed directly from the class value and
	// is thus immediately available.
	fn check_is_present_or_enqueue(&mut self,  name : &str) -> Option<JavaClassFutureRef> {
		unsafe { 
		self.ClassTableRef.unsafe_access(|table : &mut ClassTable| -> Option<JavaClassFutureRef> {
			match table.find_mut(&name.into_owned()) {
				Some(&ClassLoaded(ref elem)) => {
					return Some(JavaClassFutureRef::new(Future::from_value(Ok((*elem).clone()))));
				},
				Some(&ClassPending(ref mut chans)) => {
					let (mut port, chan) = Chan::new();
					chans.push(chan);
					return Some(JavaClassFutureRef::new(Future::from_port(port)));
				},
				None => (),
			}

			// add a new waiting queue
			table.insert(name.into_owned(), ClassPending(~[]));
			None
		})}
	}


	// ----------------------------------------------
	fn intern_add_from_classfile_bytes(&mut self, name : ~str, bytes : ~[u8]) -> 
		Result<JavaClassRef, ~str> {
		match result(|| { 
			let reader = &mut BufReader::new(bytes) as &mut Reader;
			
			let magic = reader.read_be_u32() as uint;
			if magic != 0xCAFEBABE {
				return Err(~"magic word not found");
			}

			let minor = reader.read_be_u16() as uint;
			let major = reader.read_be_u16() as uint;

			// TODO: check whether we support this format
			debug!("class file version {}.{}", major, minor);

			// 1.
			// constant pool
			let constants = match ClassLoader::load_constant_pool(reader) {
				Err(s) => return Err(s), 
				Ok(n) => n
			};

			let access = reader.read_be_u16() as uint;
	
			// 2.
			// our own name - only used for verification
			let own_name = match  ClassLoader::resolve_class_cpool_entry(constants, 
				reader.read_be_u16() as uint) {
				Err(s) => return Err(s), 
				Ok(n) => n
			};
			debug!("class name embedded in .class file is {}", own_name);
			
			// 3.
			// super class name and implemented interfaces - must be loaded
			let future_parents = match self.load_class_parents(constants, reader) {
				Err(s) => return Err(s), 
				Ok(n) => n
			};

			if future_parents.len() == 0 {
				if name != ~"java.lang.Object" && (access & ACC_INTERFACE) == 0 {
					return Err(~"Only interfaces and java.lang.Object can go without super class");
				}
			}

			// 4. class and instance fields
			let fields_count = reader.read_be_u16() as uint;

			// 5. class and instance methods
			//let methods = self.read_methods(reader, constants);
		

			/*
				// 6. class attributes - we skip them for now
				let attrs_count = reader.read_be_u16() as uint;
			*/

			return Ok(self.register_class(name, Arc::new(JavaClass::new(
				name,
				constants,
				future_parents
			))))
		}) {
			Err(e) => Err(~"ClassLoader: unexpected end-of-file or read error"),
			Ok(T) => T
		}
	}


	// ----------------------------------------------
	// Adds a class instance to the table of loaded classes 
	// and thereby marks it officially as loaded.
	fn register_class(&mut self, name : &str, class : JavaClassRef) -> JavaClassRef {
		debug!("loaded class {}", name);
		unsafe { 
			self.ClassTableRef.unsafe_access(|table : &mut ClassTable| {
				let mut entry = table.get_mut(&name.into_owned());

				match *entry {
					ClassPending(ref mut queue) => {
						for k in queue.mut_iter() {
							if !k.try_send(Ok(class.clone())) {
								debug!("failed to send class back to listener, port is hung up");
							}
						}
					},
					_ => fail!("logic error, class not marked as pending"),
				}

				*entry = ClassLoaded(class.clone());
			}) 
		};

		assert!(self.get_class(name).is_some());
		return class.clone();
	}


	// ----------------------------------------------
	// Load the portion of the .class file header that contains 
	// the constant value pool (cpool) and parse all entries
	// into proper structures.
	fn load_constant_pool(reader: &mut Reader) ->  Result<~[Constant], ~str> {
		let cpool_count = reader.read_be_u16() as uint;
		if cpool_count == 0 {
			return Err(~"invalid constant pool size");
		}

		debug!("{} constant pool entries", cpool_count - 1);
		let mut constants : ~[Constant] = ~[];

		// read constant pool
		let mut i = 1;
		while i < cpool_count {
			let tag = reader.read_u8();
			let parsed_tag : Option<ConstantPoolTags> = 
				FromPrimitive::from_u8(tag);

			let mut skip = 0;
			let maybe_centry = match parsed_tag {
				None => Err(format!("constant pool tag not recognized: {}", tag)),
				Some(c) => {
					ClassLoader::read_cpool_entry_body(c, 
						reader, 
						cpool_count as uint, 
						&mut skip
					)
				}
			};

			// if that was ok, add it to the list and advance
			match maybe_centry {
				Err(e) => return Err(e),
				Ok(centry) => {
					debug!("adding constant pool entry: {}", parsed_tag.to_str());
					constants.push(centry)
				}
			}

			i += skip + 1;
		}
		return Ok(constants);
	}


	// ----------------------------------------------
	// Loads a referenced class that is given by an entry in the cpool.
	// When calling this method, be sure to do so in a manner that 
	// avoids cyclic dependencies between classes. The primary
	// inheritance graph is a DAG and it is therefore safe but cross-references
	// between fields and exception handlers may include cycles. Use
	// load_future_class_from_cpool for this purpose.
	//
	fn load_class_from_cpool(&mut self, constants : &[Constant], index : uint)
		-> Result<JavaClassRef, ~str> {

		match ClassLoader::resolve_class_cpool_entry(
			constants, index
		) {
			Err(s) => Err(s),
			Ok(class_name) => {
				match self.add_from_classfile(class_name).await() {
					Err(s) => Err("failure loading referenced class: " + s),
					Ok(cl) => Ok(cl),
				}
			}
		}
	}


	// ----------------------------------------------
	// Obtain a future ref on a referenced class that is given by an entry
	//  in the cpool. This method does not block on loading that class and
	// is thus safe to use with cyclic references between classes.
	fn load_future_class_from_cpool(&mut self, constants : &[Constant], index : uint)
		-> JavaClassFutureRef {

		match ClassLoader::resolve_class_cpool_entry(
			constants, index
		) {
			Err(s) => JavaClassFutureRef::new_error(s),
			Ok(class_name) => self.add_from_classfile(class_name),
		}
	}


	// ----------------------------------------------
	// Load the portion of a .class file header that lists the class'
	// super class as well as all implemented interfaces and loads
	// all of them
	fn load_class_parents(&mut self, constants : &[Constant], reader: &mut Reader)  
		-> Result<~[ JavaClassRef ], ~str> {

		let mut future_parents : ~[ JavaClassRef ] = ~[];
		let parent_index = reader.read_be_u16() as uint;

		// parent_index is 0 for interfaces, and for java.lang.Object
		if parent_index != 0 {
			match self.load_class_from_cpool(constants, parent_index) {
				Err(s) => return Err("failure loading parent class: " + s),
				Ok(cl) => future_parents.push(cl)
			}
		}
				
		let ifaces_count = reader.read_be_u16() as uint;
		let mut i = 0;
		while i < ifaces_count {
			let iindex = reader.read_be_u16() as uint;
			match self.load_class_from_cpool(constants, iindex) {
				Err(s) => return Err("failure loading parent interface: " + s),
				Ok(cl) => future_parents.push(cl)
			}
			i += 1;
		}
		return Ok(future_parents);
	}


	// ----------------------------------------------
	// Loads the methods (+ static functions) section from a .class file
	fn read_methods(&self, reader: &mut Reader, constants : &[Constant]) -> Result<~[JavaMethod], ~str> {
		let mut methods : ~[JavaMethod] = ~[];
		let methods_count = reader.read_be_u16() as uint;
		for i in range(0, methods_count) {
			let access = reader.read_be_u16() as uint;
			// I definitely want some kind of a monadic "DO" notation as for Haskell
			let name = match ClassLoader::resolve_name_cpool_entry(constants, 
				reader.read_be_u16() as uint) {
				Err(s) => return Err(s),
				Ok(n) => n
			};

			let desc = match ClassLoader::resolve_name_cpool_entry(constants, 
				reader.read_be_u16() as uint) {
				Err(s) => return Err(s),
				Ok(n) => n
			};

			// scan for the "Code" attribute
			// TODO: for proper interpretation and fully secure linking we will
			// need to also process other attributes.
			let mut code_attr : Option<CodeBlock> = None;
			let attr_count = reader.read_be_u16() as uint;
			for i in range(0, attr_count) {
				let name = match ClassLoader::resolve_name_cpool_entry(constants, 
					reader.read_be_u16() as uint) {
					Err(s) => return Err(s),
					Ok(n) => n
				};

				if name == ~"Code" {
					code_attr = match self.load_code_attribute(constants, reader) {
						Err(s) => return Err(s),
						Ok(n) => Some(n),
					};
				}
			}

			if code_attr.is_none() {
				return Err(~"failed to read [Code] attribute from method attribute table");
			}

			methods.push(JavaMethod::new(name,desc,code_attr.unwrap()));
		}
		Ok(methods)
	}


	// ----------------------------------------------
	// Load a single attribute of type [Code] from a a .class file reader 
	// that is positioned correctly to be right behind the attribute head.
	// http://docs.oracle.com/javase/specs/jvms/se7/html/jvms-4.html#jvms-4.7.3
	pub fn load_code_attribute(&self, constants : &[Constant], reader: &mut Reader) -> 
		Result<CodeBlock, ~str> {

		let max_stack = reader.read_be_u16() as uint;
		let max_locals = reader.read_be_u16() as uint;
		let code_len = reader.read_be_u16() as uint;

		let codebytes = reader.read_bytes(code_len);

		// TODO: translate code bytes

		let exc_len = reader.read_be_u16() as uint;
		let mut i = 0;
		let mut exc_rec : ~[ExceptionHandler] = ~[];
		while i < exc_len {

			let start_pc = reader.read_be_u16() as uint;
			let end_pc = reader.read_be_u16() as uint;
			let handler_pc = reader.read_be_u16() as uint;
			let catch_type_index = reader.read_be_u16() as uint;

			match ClassLoader::resolve_class_cpool_entry(constants, catch_type_index) {
				Ok(cl) => {
					exc_rec.push(ExceptionHandler {
						start_pc : start_pc,
						end_pc : end_pc,
						handler_pc : handler_pc,
						catch_type : cl,
					})
				},
				Err(s) => return Err(s)
			}

			i += 1;
		}

		return Ok(CodeBlock::new(max_stack, max_locals, codebytes, exc_rec));
	}


	// ----------------------------------------------
	// Given a parsed constant pool and locate an UTF8 string entry in it
	fn resolve_name_cpool_entry(constants : &[Constant], oneb_index : uint) ->
		Result<~str,~str>	{

		assert!(oneb_index != 0 && oneb_index <= constants.len());
		match constants[oneb_index - 1] {
			CONSTANT_utf8_info(ref s) => Ok(s.clone()),
			_ => Err(~"name cpool entry is not a CONSTANT_Utf8"),
		}
	}


	// ----------------------------------------------
	// Given a parsed constant pool, locate a class entry in it and
	// resolve the UTF8 name of the class.
	fn resolve_class_cpool_entry(constants : &[Constant], oneb_index : uint) ->
		Result<~str,~str>	{

		assert!(oneb_index != 0 && oneb_index <= constants.len());

		match constants[oneb_index - 1] {
			CONSTANT_class_info(ref utf8_idx) => {
				assert!(*utf8_idx != 0 && (*utf8_idx as uint) <= constants.len());
				match constants[*utf8_idx - 1] {
					CONSTANT_utf8_info(ref s) => Ok(s.clone().replace("/",".")),
					_ => Err(~"class name cpool entry is not a CONSTANT_Utf8"),
				}
			},
			_ => Err(~"not a CONSTANT_class entry"),
		}
	}


	// ----------------------------------------------
	fn read_cpool_entry_body(tag : ConstantPoolTags, reader : &mut Reader, count : uint, 
		skip : &mut uint) -> 
		Result<Constant, ~str> {

		let mut err : Option<~str> = None;
		let cindex = || {
			// indices are in [1,count)
			let index = reader.read_be_u16();
			if index == 0 || (index as uint) >= count {
				err = Some(format!("constant pool index out of range: {}", index));
			}
			return index;
		};

		// TODO: verify the type of cross-referenced cpool entries
		// TODO: do the read_be .. variants properly raise io_error
		// for our caller to trap?

		let res = match tag {
			CONSTANT_class => 
				CONSTANT_class_info(cindex()),
			CONSTANT_fieldref => 
				CONSTANT_fieldref_info(cindex(), cindex()),
			CONSTANT_methodref => 
				CONSTANT_methodref_info(cindex(), cindex()),
			CONSTANT_ifacemethodref =>
				CONSTANT_ifacemethodref_info(cindex(), cindex()),
			CONSTANT_string => 
				CONSTANT_string_info(cindex()),
			CONSTANT_integer => 
				CONSTANT_integer_info(reader.read_be_i32()),
			CONSTANT_float => 
				CONSTANT_float_info(reader.read_be_f32()),
			CONSTANT_long => 
				CONSTANT_long_info(reader.read_be_i64()),
			CONSTANT_double => 
				CONSTANT_double_info(reader.read_be_f64()),
			CONSTANT_nameandtype => 
				CONSTANT_nameandtype_info(cindex(),cindex()),
			CONSTANT_utf8 => {
				let length = reader.read_be_u16() as uint;
				let raw = reader.read_bytes(length);

				// TODO: Java uses a "modified UTF8", which
				//  - encodes NIL as two bytes
				//  - uss two three-byte sequences to encode four byte encodings
				match from_utf8_owned(raw) {
					None => {
						err = Some(~"constant pool entry is not  valid UTF8 string");
						CONSTANT_utf8_info(~"")
					},
					Some(s) => {
						debug!("utf8 string: {}", s);
						CONSTANT_utf8_info(s)
					}
				}
			},
			CONSTANT_methodhandle => 
				CONSTANT_methodhandle_info(reader.read_u8(), cindex()),
			CONSTANT_methodtype => 
				CONSTANT_methodtype_info(cindex()),
			CONSTANT_invokedynamic => 
				CONSTANT_invokedynamic_info(reader.read_be_u16(), cindex()),
		};

		// some cpool entries take two indices. According to the spec,
		// this was "a poor choice".
		*skip = match tag {
			CONSTANT_long | CONSTANT_double => 1,
			_ => 0
		};

		match err {
			None => Ok(res),
			Some(msg) => Err(msg)
		}
	}
}

impl Clone for ClassLoader {
	fn clone(&self) -> ClassLoader {
		ClassLoader {
			classpath : self.classpath.clone(),
			ClassTableRef : self.ClassTableRef.clone(),
		}
	}
}


// Mock class loader that refuses to load any classes and instead just
// returns the "DUMMY" error string. Used for testing.
pub struct DummyClassLoader;
impl AbstractClassLoader for DummyClassLoader {
	
	fn load(&mut self, name : &str) -> JavaClassFutureRef	{
		return JavaClassFutureRef::new(Future::from_value(Err(~"DUMMY")));
	}

	fn load_from_bytes(&mut self, name : &str, bytes : ~[u8]) -> JavaClassFutureRef	{
		return JavaClassFutureRef::new(Future::from_value(Err(~"DUMMY")));
	}
}




#[cfg(test)]
pub mod tests {
	use classloader::*;
	use util::{assert_no_err};

	pub fn test_get_dummy_classloader() -> DummyClassLoader
	{
		return DummyClassLoader;
	}

	pub fn test_get_real_classloader() -> ClassLoader
	{
		return ClassLoader::new_from_string("../test/java;../rt");
	}



	#[test]
	fn test_class_loader_fail() {
		let mut cl = ClassLoader::new_from_string("");
		assert!(cl.add_from_classfile("FooClassDoesNotExist").await().is_err());
	}


	#[test]
	fn test_class_loader_good() {
		let mut cl = test_get_real_classloader();
		let mut v = cl.add_from_classfile("EmptyClass").await();
		assert_no_err(&v);

		v = cl.add_from_classfile("FieldAccess").await();
		assert_no_err(&v);

		v = cl.add_from_classfile("InterfaceImpl").await();
		assert_no_err(&v);
	}


	#[test]
	fn test_class_loader_concurrent_loading() {
		let mut cl_outer = test_get_real_classloader();

		let (port1, chan1) = Chan::new();
		let (port2, chan2) = Chan::new();

		let cl = cl_outer.clone();
		do spawn {
			let mut v = cl.clone().add_from_classfile("EmptyClass").await();
			assert_no_err(&v);
			chan1.send(1);
		}

		let cl2 = cl_outer.clone();
		do spawn {
			let mut v = cl2.clone().add_from_classfile("EmptyClass").await();
			assert_no_err(&v);
			chan2.send(1);	
		}

		cl_outer.add_from_classfile("EmptyClass");
		port1.recv();
		port2.recv();

		assert!(cl_outer.get_class("EmptyClass").is_some());
	}
}


