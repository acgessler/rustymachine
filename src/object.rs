
use std::ops::{Index};
use class::{JavaClassRef};


// A JavaObject instance represents an alive Java object. Instances
// of Java objects are reference counted.

pub struct JavaObject {
	priv ref_count : uint,
	priv jclass : JavaClassRef,
	priv fields : ~[u32],
}


impl JavaObject {

	// ----------------------------------------------
	// Construct a JavaObject and provide constant-field
	// initialization according to the runtime-layout
	// table of that class. No constructor code is 
	// executed.
	pub fn new(jclass : JavaClassRef) -> JavaObject
	{
		JavaObject {
			ref_count : 1,
			jclass : jclass,
			fields : ~[]
		}
		// TODO: field initialization
	}


	// ----------------------------------------------
	pub fn get_class(&self) -> JavaClassRef 
	{
		self.jclass.clone()
	}
}


impl Index<uint, u32> for JavaObject {
    fn index(&self, idx: &uint) -> u32 {
    	assert!(*idx < self.fields.len());
    	self.fields[*idx]
    }
}



