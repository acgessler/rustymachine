extern mod std;

use std::hashmap::HashMap;
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

use extra::future::Future;
use extra::arc::{Arc, MutexArc};

use def::Constant;
use method::JavaMethod;
use field::JavaField;



// shared ref to a java class def
pub type JavaClassRef = Arc<JavaClass>;


// future ref to a java class. This is used during loading
// both to emphasize asynchronous loading and for avoiding deadlocks
// due to cyclic dependencies between classes.
pub struct JavaClassFutureRef
{
	priv inner : Future<Result<JavaClassRef,~str>>,
}


impl JavaClassFutureRef
{
	// ----------------------------------------------
	pub fn new(val : Future<Result<JavaClassRef,~str>>) -> JavaClassFutureRef
	{
		JavaClassFutureRef {
			inner : val,
		}
	}

	// ----------------------------------------------
	pub fn new_error(s : &str) -> JavaClassFutureRef
	{
		JavaClassFutureRef {
			inner : Future::from_value(Err(s.into_owned()))
		}
	}

	// ----------------------------------------------
	// Awaits the class being loaded and returns the result
	pub fn await(&mut self) -> Result<JavaClassRef,~str> {
		return self.inner.get_ref().clone();
	}

	// ----------------------------------------------
	// WARN: fail!s if the java class could not be loaded
	pub fn unwrap_all(&mut self) -> ~JavaClassRef {
		return ~self.await().unwrap();
	}
}


// internal representation of a loaded java class.
// TODO: add different states - linked y/n etc
pub struct JavaClass {
	priv name : ~str,
	priv attrs : uint,
	priv constants : ~[Constant],
	priv parents : ~[ JavaClassRef ],
	priv methods : ~HashMap<~str, ~JavaMethod>,

	// TODO: runtime layout table constructed for instance fields and class fields
}


impl JavaClass {

	// ----------------------------------------------
	pub fn new(name : &str, constants : ~[Constant], parents : ~[ JavaClassRef ] ) 
	-> JavaClass 
	{
		JavaClass { 
			name: name.into_owned(), 
			attrs: 0, 
			methods : ~HashMap::with_capacity(16),
			constants : constants,
			parents : parents,
		}
	}


	// ----------------------------------------------
	pub fn get_name<'a>(&'a self) -> &'a ~str {
		return &self.name
	} 
}

