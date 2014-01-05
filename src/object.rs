
use std::ops::{Index};
use class::{JavaClassRef};
use monitor::{JavaMonitor};

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
	// to own the object. This list is part of the object
	// itself and transferred between threads.
	//
	// This list is independent of the object's monitor's list
	// of threads that are currently blocking on the monitor.
	priv waiters : ~[uint],

	// The monitor object that guards synchronized object access
	priv monitor : JavaMonitor,
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
			monitor : JavaMonitor::new()
		}
		// TODO: field initialization
	}

	// ----------------------------------------------
	// Get the oid (object id) of the object. The oid does
	// not change during lifetime of the object. oids may 
	// be recycled after the object's death though.
	#[inline]
	pub fn get_oid(&self) -> JavaObjectId {
		self.oid
	}

	// ----------------------------------------------
	// Get the underlying Java type of the object
	#[inline]
	pub fn get_class(&self) -> JavaClassRef {
		self.jclass.clone()
	}

	// ----------------------------------------------
	// Use LocalHeap::add_ref() instead
	#[inline]
	pub fn intern_add_ref(&mut self) {
		self.ref_count += 1;
	}

	// ----------------------------------------------
	// Use LocalHeap::release() instead
	// Returns whether the object's ref count is nonzero
	#[inline]
	pub fn intern_release(&mut self) -> bool {
		assert!(self.ref_count >= 1);
		self.ref_count -= 1;
		self.ref_count != 0
	}

	// ----------------------------------------------
	// Access the monitor of the object
	#[inline]
	pub fn monitor<'t>(&'t self) -> &'t JavaMonitor {
		&self.monitor
	}

	#[inline]
	pub fn monitor_mut<'t>(&'t mut self) -> &'t mut JavaMonitor {
		&mut self.monitor
	}
}


impl Index<uint, u32> for JavaObject {
    fn index(&self, idx: &uint) -> u32 {
    	assert!(*idx < self.fields.len());
    	self.fields[*idx]
    }
}



