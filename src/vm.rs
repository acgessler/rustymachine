
use std::unstable::atomics::{atomic_add, AcqRel};

use std::hashmap::{HashMap};

use objectbroker::{ObjectBrokerMessage, ObjectBroker, OB_REGISTER};


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
	priv id : uint,

	// heap objects currently owned by this thread context
	priv owned_objects : HashMap<uint, ~[u32]>,

	priv broker_port : Port<ObjectBrokerMessage>,
	priv broker_chan : SharedChan<ObjectBrokerMessage>,

	priv opstack : ~[u32],
	priv locals : ~[u32],

	priv frames : ~[FrameInfo],
}

static mut ThreadContextIdCounter : uint = 0;

impl ThreadContext {


	// ----------------------------------------------
	pub fn new(broker_chan : SharedChan<ObjectBrokerMessage>) -> ThreadContext 
	{
		// generate an unique thread id
		let id = unsafe {
			atomic_add(&mut ThreadContextIdCounter, 1, AcqRel)
		};

		// register with the object broker
		let (port, chan) = Chan::new();
		broker_chan.send(OB_REGISTER(id, chan));

		ThreadContext {
			id : id,

			owned_objects : HashMap::with_capacity(1024),
			broker_port : port,
			broker_chan : broker_chan,

			opstack : ~[],
			locals : ~[],
			frames : ~[],
		}
	}


	// ----------------------------------------------
	pub fn execute() {
		loop {
			//op();
			//heap.update();
		}
	}


	// IMPL

	// ----------------------------------------------
	#[inline]
	fn op() {

	}

	/*

	#[inline]
	fn access_object(&self, id : uint, fn a(~u32) -> ~u32) -> ~u32 {
		// check local hashmap first
		//match self.owned_objects.find(id) {
		//	Some(ref obj) => 
		//}
	} */
}














