extern mod std;

use std::hashmap::HashMap;

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

