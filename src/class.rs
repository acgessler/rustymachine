use std::hashmap::HashMap;
mod method;


pub struct JavaClass {
	priv name : ~str,
	priv attrs : uint,
	priv methods : HashMap<~str, ~method::JavaMethod>,
}


impl JavaClass {

	pub fn new() -> JavaClass {
		JavaClass { 
			name: ~"", 
			attrs: 0, 
			methods : HashMap::with_capacity(16)
		}
	}
}