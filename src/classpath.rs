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

extern mod std;
extern mod extra;

use extra::arc::{Arc};

use std::io::{File,result, IoError};

use std::path::{PosixPath};

pub struct ClassPath {
	priv elems : Arc<~[~str]>,
}


impl ClassPath  {

	// ----------------------------------------------
	/** Convert from semicolon-separated list of paths to a ClassPath instance */
	pub fn new_from_string(invar : &str) -> ClassPath 
	{
		// current folder is always included
		let mut v = ~[~"."];

		// TODO: how to construct a vector directly from an iter?
		for s in invar.split_str(";")
			.map(|s : &str| { s.trim().to_owned() }) 
			.filter(|s : &~str| {s.len() > 0}){

			v.push(s);
		}
		ClassPath {
			elems : Arc::new(v)
		}
	}


	// ----------------------------------------------
	pub fn get_paths<'a>(&'a self) -> &'a ~[~str]
	{
		return self.elems.get();
	}


	// ----------------------------------------------
	/** Locate a given class (given by fully qualified name) and return
	 *  the bytes of its classfile. */
	pub fn locate_and_read(&self, name : &str) -> Option<~[u8]>
	{
		let cname = name.to_owned();
		let pname = cname.replace(&".", "/") + ".class";
		for path in self.elems.get().iter() {
				
			match result(|| { 
				let p = *path + "/" + pname;
				debug!("locate class {}, trying path {}", cname, p);
				File::open(&PosixPath::new(p)).read_to_end()
			}) {
				Err(e) => continue,
				Ok(bytes) => {
					debug!("found .class file");
					return Some(bytes)
				}
			};
		}
		return None
	}
}


impl Clone for ClassPath {
	fn clone(&self) -> ClassPath {
		ClassPath {
			elems : self.elems.clone()
		}
	}
}


#[cfg(test)]
mod tests {
	use classpath::*;

	#[test]
	fn test_class_path_decomposition() {
		let cp = ClassPath::new_from_string("~/some/other/bar; /bar/baz;dir ;");
		assert_eq!(*cp.get_paths(),~[~".",~"~/some/other/bar", ~"/bar/baz", ~"dir"]);
		assert_eq!(*cp.get_paths(),~[~".",~"~/some/other/bar", ~"/bar/baz", ~"dir"]);
	}

}