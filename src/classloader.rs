extern mod extra;
extern mod std;

use std::hashmap::HashMap;
use std::path::PosixPath;

use std::io::{File,result, IoError};
use std::io::mem::BufReader;

use std::num::FromPrimitive;

use std::str::from_utf8_owned_opt;

use extra::future::Future;
use extra::arc::Arc;


mod util;
mod class;
mod def;


fn build_classpath(invar : &str) -> ~[~str] {
	// current folder is always included
	let mut v = ~[~"."];

	// TODO: how to construct a vector directly from an iter?
	for s in invar.split_str(";")
		.map(|s : &str| { s.trim().to_owned() }) 
		.filter(|s : &~str| {s.len() > 0}){

		v.push(s);
	}
	return v;
}


pub struct ClassLoader {
	priv classpath : ~[~str],
	priv classes : HashMap<~str, ~class::JavaClass>
}


static INITIAL_CLASSLOADER_CAPACITY : uint = 1024;

impl ClassLoader {

	// ----------------------------------------------
	// Constructs a ClassLoader given a CLASSPATH that consists of one or 
	// more .class search paths separated by semicolons.
	pub fn new(classpath : &str) -> ~ClassLoader {
		~ClassLoader {
			classpath : build_classpath(classpath),
			classes : HashMap::with_capacity(INITIAL_CLASSLOADER_CAPACITY),
		}
	}


	// ----------------------------------------------
	// Loads a class given its fully qualified class name.
	//
	// This triggers recursive loading of dependent classes.
	// 
	// add_from_classfile("de.fruits.Apple") loads <class-path>/de/fruits/Apple.class
	pub fn add_from_classfile(self, name : &str) -> 
		Future<
			Result<Arc<class::JavaClass>,~str>
		>
	{
		let cname = name.to_owned();
		let pname = cname.replace(&".", "/") + ".class";
		do Future::spawn {
			for path in self.classpath.iter() {
				
				match result(|| { 
					let p = *path + "/" + pname;
					debug!("load class {}, trying path {}", cname, p);
					File::open(&PosixPath::new(p)).read_to_end()
				}) {
					Err(e) => continue,
					Ok(bytes) => {
						debug!("found .class file");
						return self.add_from_classfile_bytes(bytes)
							.unwrap() 
					}
				};
			}
			return Err(~"failed to locate class file for " + cname);
		}
	}


	// IMPL


	// ----------------------------------------------
	fn add_from_classfile_bytes(self, bytes : ~[u8]) -> 
		Future<
			Result<Arc<class::JavaClass>,~str>
		> 
	{
		do Future::spawn {
			match result(|| { 
				let mut reader = BufReader::new(bytes);
				
				let magic = reader.read_be_u32() as uint;
				if magic != 0xCAFEBABE {
					return Err(~"magic word not found");
				}

				let minor = reader.read_be_u16() as uint;
				let major = reader.read_be_u16() as uint;

				// TODO: check whether we support this format
				debug!("class file version {}.{}", major, minor);

				let cpool_count = reader.read_be_u16() as uint;
				if cpool_count == 0 {
					return Err(~"invalid constant pool size");
				}

				debug!("{} constant pool entries", cpool_count - 1);
				let mut constants : ~[def::Constant] = ~[];

				// read constant pool
				let mut i = 1;
				while i < cpool_count {
					let tag = reader.read_u8();
					let parsed_tag : Option<def::ConstantPoolTags> = 
						FromPrimitive::from_u8(tag);

					let mut skip = 0;
					let maybe_centry = match parsed_tag {
						None => Err(format!("constant pool tag not recognized: {}", tag)),
						Some(c) => {
							ClassLoader::read_cpool_entry_body(c, 
								&mut reader as &mut Reader, 
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

				let access = reader.read_be_u16() as uint;
				let this_class = reader.read_be_u16() as uint;
				let super_class = reader.read_be_u16() as uint;
				let ifaces_count = reader.read_be_u16() as uint;

				// read interfaces

				let fields_count = reader.read_be_u16() as uint;
				let methods_count = reader.read_be_u16() as uint;

				// read methods

				let attrs_count = reader.read_be_u16() as uint;

				// read attributes
				return Ok(Arc::new(class::JavaClass::new()))
			}) {
				Err(e) => Err(~"classloader: unexpected end-of-file or read error"),
				Ok(T) => T
			}
		}
	}


	// ----------------------------------------------
	fn read_cpool_entry_body(tag : def::ConstantPoolTags, reader : &mut Reader, count : uint, 
		skip : &mut uint) -> 
		Result<def::Constant, ~str> 
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
					Some(s) => def::CONSTANT_utf8_info(s)
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

#[test]
fn test_class_path_decomposition() {
	let cp = build_classpath("~/some/other/bar; /bar/baz;dir ;");
	assert_eq!(cp,~[~".",~"~/some/other/bar", ~"/bar/baz", ~"dir"]);
}

#[test]
fn test_class_loader_fail() {
	let cl = ClassLoader::new("");
	assert!(cl.add_from_classfile("FooClassDoesNotExist").unwrap().is_err());
}


#[test]
fn test_class_loader_good() {
	let cl = ClassLoader::new("../test/java");
	let v = cl.add_from_classfile("EmptyClass").unwrap();
	util::assert_no_err(v);
}

