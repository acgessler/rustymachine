use std::hashmap::HashMap;

use extra::future::Future;
use extra::arc::Arc;

use def::Constant;
use method::JavaMethod;
use field::JavaField;


type JavaClassRef = Arc<JavaClass>;


pub struct JavaClass {
	priv name : ~str,
	priv attrs : uint,
	priv constants : ~[Constant],
	priv parents : ~[ JavaClassRef ],
	priv methods : ~HashMap<~str, ~JavaMethod>,
}


impl JavaClass {

	// ----------------------------------------------
	pub fn new(constants : ~[Constant], parents : ~[ JavaClassRef ] ) 
	-> JavaClass 
	{
		JavaClass { 
			name: ~"", 
			attrs: 0, 
			methods : ~HashMap::with_capacity(16),
			constants : constants,
			parents : parents
		}
	}


	// ----------------------------------------------
	pub fn get_name<'a>(&'a self) -> &'a ~str {
		return &self.name
	} 
}

