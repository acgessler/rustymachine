extern mod extra;
extern mod std;

use std::hashmap::HashMap;
use std::path::PosixPath;

use std::io::mem::BufReader;
use std::io::{result, IoError};

use std::num::FromPrimitive;

use std::str::from_utf8_owned_opt;

use extra::future::Future;
use extra::arc::{Arc, RWArc, MutexArc};


use util::assert_no_err;
use def::*;
use class::{JavaClass};
use classpath::{ClassPath};



// shared ref to a java class def
pub type JavaClassRef = Arc<JavaClass>;

// future ref to a java class
pub type JavaClassFutureRef = Future<Result<JavaClassRef,~str>>;

// table of java classes indexed by fully qualified name
pub type ClassTable = HashMap<~str, JavaClassRef>;
pub type ClassTableRef = MutexArc<ClassTable>;



static INITIAL_CLASSLOADER_CAPACITY : uint = 1024;



/** */
pub trait AbstractClassLoader {
	fn load(&mut self, name : &str) -> JavaClassFutureRef;
}



// Mock class loader
pub struct DummyClassLoader;
impl AbstractClassLoader for DummyClassLoader {
	fn load(&mut self, name : &str) -> JavaClassFutureRef
	{
		return Future::from_value(Err(~"DUMMY"));
	}
}



// Real, user-facing class loader that maintains global state
pub struct ClassLoader {
	priv inner : ClonableClassLoader
}

impl AbstractClassLoader for ClassLoader {
	fn load(&mut self, name : &str) -> JavaClassFutureRef
	{
		return self.add_from_classfile(name);
	}
}


impl ClassLoader {

	// ----------------------------------------------
	// Constructs a ClassLoader given a CLASSPATH that consists of one or 
	// more .class search paths separated by semicolons.
	pub fn new(classpath : &str) -> ClassLoader {
		ClassLoader {
			inner: ClonableClassLoader::new(
				ClassPath::new_from_string(classpath),
				MutexArc::new(HashMap::with_capacity(INITIAL_CLASSLOADER_CAPACITY))
			),
		}
	}


	// ----------------------------------------------
	// Checks if a class with a given name is currently loaded. Note that
	// class loading is inherently asynchronous.
	pub fn get_class(&self, name : &str) -> Option<JavaClassRef>
	{
		return self.inner.get_class(name);
	}


	// ----------------------------------------------
	// Loads a class given its fully qualified class name.
	//
	// This triggers recursive loading of dependent classes.
	// 
	// add_from_classfile("de.fruits.Apple") loads <class-path>/de/fruits/Apple.class
	pub fn add_from_classfile(&mut self, name : &str) -> JavaClassFutureRef
	{
		return self.inner.clone().add_from_classfile(name);
	}


	// ----------------------------------------------
	pub fn get_classpath(&self) -> ClassPath
	{
		return self.inner.classpath.clone();
	}


	// internal
	// ----------------------------------------------
	fn get_class_table(&self) -> ClassTableRef
	{
		return self.inner.ClassTableRef.clone();
	}

}



// The difference between ClassLoader and ClonableClassLoader is that the latter
// is clonable and only holds Arcs to (mutable) shared state. During class loading,
// instances of ClonableClassLoader are copied and send to futures that do the
// actual loading.
//
// ClassLoader is the API that is visible to the user as it may encompass further 
// utilities or members, and does not bear the ambiguity of being copyable.
struct ClonableClassLoader {
	priv classpath : ClassPath,
	priv ClassTableRef : ClassTableRef
}


impl AbstractClassLoader for ClonableClassLoader {
	fn load(&mut self, name : &str) -> JavaClassFutureRef
	{
		return self.clone().add_from_classfile(name);
	}
}


impl ClonableClassLoader {

	// ----------------------------------------------
	pub fn new(classpath : ClassPath, ClassTableRef : ClassTableRef) -> ClonableClassLoader {
		ClonableClassLoader {
			classpath : classpath,
			ClassTableRef : ClassTableRef,
		}
	}


	// ----------------------------------------------
	pub fn get_class(&self, name : &str) -> Option<JavaClassRef>
	{
		let cname = name.into_owned();
		unsafe { 
			self.ClassTableRef.unsafe_access(|table : &mut ClassTable| {
				match table.find(&cname) {
					Some(ref elem) => Some((*elem).clone()),
					None => None
				}
			})
		}
	}

	
	// ----------------------------------------------
	pub fn add_from_classfile(self, name : &str) -> JavaClassFutureRef
	{
		// do nothing if the class is already loaded
		match self.get_class(name) {
			Some(class) => {
				return Future::from_value(Ok(class));
			},
			None => ()
		}

		let cname = name.into_owned();

		do Future::spawn {
			match self.classpath.locate_and_read(cname) {
				None => Err(~"failed to locate class file for " + cname),
				Some(bytes) => {
					self.add_from_classfile_bytes(cname, bytes)
				}
			}
		}
	}


	// IMPL


	// ----------------------------------------------
	fn add_from_classfile_bytes(mut self, name : ~str, bytes : ~[u8]) -> 
		Result<JavaClassRef, ~str>
	{
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
			let maybe_cpool = ClonableClassLoader::load_constant_pool(reader);
			match maybe_cpool {
				Err(s) => return Err(s), _ => ()
			}

			let constants = maybe_cpool.unwrap();
			let access = reader.read_be_u16() as uint;
	
			// 2.
			// our own name - only used for verification
			let maybe_name = ClonableClassLoader::resolve_class_cpool_entry(
				constants, reader.read_be_u16() as uint);
			match maybe_name {
				Err(s) => return Err(s), _ => ()
			}

			debug!("class name embedded in .class file is {}", maybe_name.unwrap());
			
			// 3.
			// super class name and implemented interfaces - must be loaded
			let maybe_parents = self.load_class_parents(
				constants, reader
			);

			match maybe_parents {
				Err(s) => return Err(s), _ => ()
			}

			let future_parents = maybe_parents.unwrap();
			if future_parents.len() == 0 {
				if name != ~"java.lang.Object" && (access & ACC_INTERFACE) == 0 {
					return Err(~"Only interfaces and java.lang.Object can go without super class");
				}
			}

			// 4. class and instance fields
			let fields_count = reader.read_be_u16() as uint;

			// 5. class and instance methods
			let methods_count = reader.read_be_u16() as uint;

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
			Err(e) => Err(~"classloader: unexpected end-of-file or read error"),
			Ok(T) => T
		}
	}


	// ----------------------------------------------
	// Adds a class instance to the table of loaded classes 
	// and thereby marks it officially as loaded.
	fn register_class(&self, name : &str, class : JavaClassRef) -> JavaClassRef
	{
		let cname = name.into_owned();


		let res = unsafe { 
			self.ClassTableRef.unsafe_access(|table : &mut ClassTable| {
				(*table.find_or_insert(cname.clone(), class.clone())).clone()
			}) 
		};

		debug!("loaded class {}", name);
		assert!(self.get_class(name).is_some());

		return res;
	}


	// ----------------------------------------------
	// Load the portion of the .class file header that containts 
	// the constant value pool (cpool) and parse all entries
	// into proper structures.
	fn load_constant_pool(reader: &mut Reader) ->  Result<~[Constant], ~str>
	{
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
					ClonableClassLoader::read_cpool_entry_body(c, 
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
	// Load the portion of a .class file header that lists the class'
	// super class as well as all implemented interfaces, loads all
	// of them and returns a list of future classes.
	fn load_class_parents(&self, constants : &[Constant], reader: &mut Reader)  
		-> Result<~[ JavaClassRef ], ~str>
	{
		let mut future_parents : ~[ JavaClassRef ] = ~[];

		let parent_index = reader.read_be_u16() as uint;

		// parent_index is 0 for interfaces, and for java.lang.Object
		if parent_index != 0 {
			match ClonableClassLoader::resolve_class_cpool_entry(
				constants, parent_index
			) {
				Err(s) => return Err(s),
				Ok(super_class_name) => {
					match self.clone().add_from_classfile(super_class_name).unwrap() {
						Err(s) => return Err("failure loading parent class: " + s),
						Ok(cl) => future_parents.push(cl),
					}
				}
			}
		}
				
		let ifaces_count = reader.read_be_u16() as uint;
		let mut i = 0;
		while i < ifaces_count {
			match ClonableClassLoader::resolve_class_cpool_entry(
				constants, reader.read_be_u16() as uint
			) {
				Err(s) => return Err(s),
				Ok(iface_name) => {
					match self.clone().add_from_classfile(iface_name).unwrap() {
						Err(s) => return Err("failure loading interface: " + s),
						Ok(cl) => future_parents.push(cl),
					}
				}
			}
			i += 1;
		}
		return Ok(future_parents);
	}


	// ----------------------------------------------
	// Given a parsed constant pool, locate a class entry in it and
	// resolve the UTF8 name of the class.
	fn resolve_class_cpool_entry(constants : &[Constant], oneb_index : uint) ->
		Result<~str,~str>
	{
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
		Result<Constant, ~str> 
	{
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
				let s = from_utf8_owned_opt(raw);
				match s {
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

impl Clone for ClonableClassLoader {
	fn clone(&self) -> ClonableClassLoader {
		ClonableClassLoader {
			classpath : self.classpath.clone(),
			ClassTableRef : self.ClassTableRef.clone(),
		}
	}
}




pub fn test_get_dummy_classloader() -> DummyClassLoader
{
	return DummyClassLoader;
}

pub fn test_get_real_classloader() -> ClassLoader
{
	return ClassLoader::new("../test/java;../rt");
}



#[test]
fn test_class_loader_fail() {
	let mut cl = ClassLoader::new("");
	assert!(cl.add_from_classfile("FooClassDoesNotExist").unwrap().is_err());
}


#[test]
fn test_class_loader_good() {
	let mut cl = test_get_real_classloader();
	let mut v = cl.add_from_classfile("EmptyClass").unwrap();
	assert_no_err(&v);

	v = cl.add_from_classfile("FieldAccess").unwrap();
	assert_no_err(&v);

	v = cl.add_from_classfile("InterfaceImpl").unwrap();
	assert_no_err(&v);
}

