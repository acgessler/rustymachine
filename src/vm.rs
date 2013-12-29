


// A FrameInfo represents 
pub struct FrameInfo {
	// not necessaryily up-to-date for top of stack
	pc : uint,
	pc_opstack : uint,
	pc_locals : uint
}


// A context of execution in the VM, typically associated with, but
// not necessarily limited to, interpreting a java.lang.Thread
// instance.
pub struct ThreadContext {
	// id of this thread. Unique across all threads as it
	// is drawn from an atomic counter.
	id : uint,

	// heap objects currently owned by this thread context
	owned_objects : HashMap<uint, ~[u8]>,

	broker_port : Port<ObjectBrokerRequest>,
	broker_chan : Chan<ObjectBrokerResponse>,

	opstack : ~[u8],
	locals : ~[u8],

	frames : ~[FrameInfo],
}



impl ThreadContext {

	pub fn execute() {

	}


	#[inline]
	fn access_object(&self, id : uint, fn a(~u8) -> ~u8) -> ~u8 {
		// check local hashmap first
		match self.owned_objects.find(id) {
			Some(ref obj) => 
		}
	}
}














