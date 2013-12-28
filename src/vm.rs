

use std::comm::{SharedChan, Port};



pub enum ObjectBrokerIncomingRequest {
	OB_IN_RQ_ADD_REF(uint, uint),
	OB_IN_RQ_RELEASE(uint, uint),
	OB_IN_RQ_WHO_OWNS(uint, uint),
	OB_IN_RQ_OWN(uint, uint),
	OB_IN_RQ_DISOWN(uint, uint)
}


pub enum ObjectBrokerOutgoingResponse {
	OB_OUT_RE_OBJ(uint, ~[u8])
}


// The ObjectBroker handles ownership for concurrently accessed
// objects. At every time, an object has one well-defined owner.
// If a thread needs access to an object that it does not currently
// own, it submits a OB_IRQ_OWN message to the object broker, which
// in turn asks the thread who owns the object to relinquish it using
// a OB_IN_RQ_DISOWN message. The owning thread gives up ownership and
// sends a OB_OUT_RE_OBJ message to the broker, which forwards it to
// the original thread.
//
// The ObjectBroker keeps a HM of object ids mapped to their owning
// thread ids.
//
// When a thread dies, it forwards all of its alive objects to the 
// object broker using a OB_OUT_RE_OBJ message. The broker, in turn, 
// keeps those objects internally until another thread demands to
// own them.
pub struct ObjectBroker {
	objects_with_owners : HashMap<uint, uint>,

	in_port : Port<ObjectBrokerRequest>,
	out_chan : HashMap<uint, Chan<ObjectBrokerResponse>>

	in_shared_chan : SharedChan<ObjectBrokerRequest>,
}


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
}




