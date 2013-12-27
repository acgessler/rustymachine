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


use def::{ConstantPoolTags, Constant};
use class::{JavaClass};
use classpath::{ClassPath};

mod def;
mod util;
mod class;
mod classpath;


// shared ref to a java class def
type JavaClassRef = Arc<JavaClass>;

// future ref to a java class
type JavaClassFutureRef = Future<Result<JavaClassRef,~str>>;

// table of java classes indexed by fully qualified name
type ClassTable = HashMap<~str, JavaClassRef>;
type ClassTableRef = MutexArc<ClassTable>;



static INITIAL_CLASSLOADER_CAPACITY : uint = 1024;



pub struct ClassLoader {
	priv inner : ClonableClassLoader
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



struct ClonableClassLoader {
	priv classpath : ClassPath,
	priv ClassTableRef : ClassTableRef
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
							.unwrap() 
				}
			}
		}
	}


	// IMPL


	// ----------------------------------------------
	fn add_from_classfile_bytes(mut self, name : ~str, bytes : ~[u8]) -> JavaClassFutureRef
	{
		do Future::spawn {
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

				// constant pool
				match ClonableClassLoader::load_constant_pool(reader) {
					Err(s) => return Err(s),
					Ok(constants) => {
						let access = reader.read_be_u16() as uint;
				
						// our own name - only used for verification
						match ClonableClassLoader::resolve_class_cpool_entry(
							constants, reader.read_be_u16() as uint
						) {
							Err(s) => return Err(s),
							Ok(name) => {
								debug!("class name embedded in .class file is {}", name);
							}
						}
						
						// super class name and implemented interfaces - must be loaded
						match self.load_class_parents(
							constants, reader
						) {
							Err(s) => return Err(s),
							Ok(future_parents) => {
								// TODO:
								let fields_count = reader.read_be_u16() as uint;
								let methods_count = reader.read_be_u16() as uint;

								// read methods

								let attrs_count = reader.read_be_u16() as uint;

								return Ok(self.register_class(name, Arc::new(JavaClass::new(
									constants,
									future_parents
								))))
							}
						}
					}
				}
			}) {
				Err(e) => Err(~"classloader: unexpected end-of-file or read error"),
				Ok(T) => T
			}
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
		match ClonableClassLoader::resolve_class_cpool_entry(
			constants, reader.read_be_u16() as uint
		) {
			Err(s) => Err(s),
			Ok(super_class_name) => {

				let mut future_parents : ~[ JavaClassRef ] = ~[];
				match self.clone().add_from_classfile(super_class_name).unwrap() {
					Err(s) => return Err("failure loading parent class: " + s),
					Ok(cl) => future_parents.push(cl),
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
		}
	}


	// ----------------------------------------------
	// Given a parsed constant pool, locate a class entry in it and
	// resolve the UTF8 name of the class.
	fn resolve_class_cpool_entry(constants : &[Constant], oneb_index : uint) ->
		Result<~str,~str>
	{
		assert!(oneb_index != 0 && oneb_index <= constants.len());

		match constants[oneb_index - 1] {
			def::CONSTANT_class_info(ref utf8_idx) => {
				assert!(*utf8_idx != 0 && (*utf8_idx as uint) <= constants.len());
				match constants[*utf8_idx - 1] {
					def::CONSTANT_utf8_info(ref s) => Ok(s.clone()),
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
			def::CONSTANT_class => 
				def::CONSTANT_class_info(cindex()),
			def::CONSTANT_fieldref => 
				def::CONSTANT_fieldref_info(cindex(), cindex()),
			def::CONSTANT_methodref => 
				def::CONSTANT_methodref_info(cindex(), cindex()),
			def::CONSTANT_ifacemethodref =>
				def::CONSTANT_ifacemethodref_info(cindex(), cindex()),
			def::CONSTANT_string => 
				def::CONSTANT_string_info(cindex()),
			def::CONSTANT_integer => 
				def::CONSTANT_integer_info(reader.read_be_i32()),
			def::CONSTANT_float => 
				def::CONSTANT_float_info(reader.read_be_f32()),
			def::CONSTANT_long => 
				def::CONSTANT_long_info(reader.read_be_i64()),
			def::CONSTANT_double => 
				def::CONSTANT_double_info(reader.read_be_f64()),
			def::CONSTANT_nameandtype => 
				def::CONSTANT_nameandtype_info(cindex(),cindex()),
			def::CONSTANT_utf8 => {
				let length = reader.read_be_u16() as uint;
				let raw = reader.read_bytes(length);

				// TODO: Java uses a "modified UTF8", which
				//  - encodes NIL as two bytes
				//  - uss two three-byte sequences to encode four byte encodings
				let s = from_utf8_owned_opt(raw);
				match s {
					None => {
						err = Some(~"constant pool entry is not  valid UTF8 string");
						def::CONSTANT_utf8_info(~"")
					},
					Some(s) => {
						debug!("utf8 string: {}", s);
						def::CONSTANT_utf8_info(s)
					}
				}
			},
			def::CONSTANT_methodhandle => 
				def::CONSTANT_methodhandle_info(reader.read_u8(), cindex()),
			def::CONSTANT_methodtype => 
				def::CONSTANT_methodtype_info(cindex()),
			def::CONSTANT_invokedynamic => 
				def::CONSTANT_invokedynamic_info(reader.read_be_u16(), cindex()),
		};

		// some cpool entries take two indices. According to the spec,
		// this was "a poor choice".
		*skip = match tag {
			def::CONSTANT_long | def::CONSTANT_double => 1,
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


#[test]
fn test_class_loader_fail() {
	let mut cl = ClassLoader::new("");
	assert!(cl.add_from_classfile("FooClassDoesNotExist").unwrap().is_err());
}


#[test]
fn test_class_loader_good() {
	let mut cl = ClassLoader::new("../test/java");
	let v = cl.add_from_classfile("EmptyClass").unwrap();
	util::assert_no_err(v);
}

