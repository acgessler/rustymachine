
use std::ops::{Index};
use class::{JavaClassRef};

// Type used for referencing objects. A 64 bit integer is used
// to ensure that we never run out of ids.
pub type JavaObjectId = u64;


// A JavaObject instance represents an alive Java object. Instances
// of Java objects are reference counted. At every time, an object
// has a well-defined owning ThreadContext. 

pub struct JavaObject {
	// unique, life-time id of the object. Objects on the heap
	// are keyed on their oid and a reference to a JavaObject
	// is made by holding the id guarded by AddRef/Release.
	priv oid : JavaObjectId,

	priv ref_count : uint,
	priv jclass : JavaClassRef,
	priv fields : ~[u32],

	// List of threads (by their uint id) currently waiting
	// to acquire the object. This list is part of the object
	// itself and transferred between threads.
	//
	// This list is a superset of the object's monitor's list
	// of threads that are currently blocking on the monitor.
	priv waiters : ~[uint],
}


impl JavaObject {

	// ----------------------------------------------
	// Construct a JavaObject and provide constant-field
	// initialization according to the runtime-layout
	// table of that class. No constructor code is 
	// executed.
	//
	// Do not invoke this method directly, instead use
	// LocalHeap::new_XXX.
	//
	// The intial refcount for objects is 1.
	pub fn new(jclass : JavaClassRef, oid : JavaObjectId) -> JavaObject
	{
		JavaObject {
			oid : oid,
			ref_count : 1,
			jclass : jclass,
			fields : ~[],
			waiters : ~[],
		}
		// TODO: field initialization
	}

	#[inline]
	pub fn get_oid(&self) -> JavaObjectId {
		self.oid
	}

	#[inline]
	pub fn get_class(&self) -> JavaClassRef {
		self.jclass.clone()
	}
}


impl Index<uint, u32> for JavaObject {
    fn index(&self, idx: &uint) -> u32 {
    	assert!(*idx < self.fields.len());
    	self.fields[*idx]
    }
}



