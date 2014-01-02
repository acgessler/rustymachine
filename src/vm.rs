
use std::unstable::atomics::{atomic_add, AcqRel};

use std::hashmap::{HashMap};

use objectbroker::{ObjectBrokerMessage, ObjectBroker, OB_REGISTER};

use localheap::{LocalHeap};


// A FrameInfo represents 
pub struct FrameInfo {
	// not necessaryily up-to-date for top of stack
	pc : uint,
	pc_opstack : uint,
	pc_locals : uint
}

/*
// Messages exchanged between the VM and individual ThreadContext's
pub enum VMControlMessage {
	// interrupt a specific thread
	VM_CONTROL_INTERRUPT(id),

	// System.exit(id)
	VM_CONTROL_EXIT(id),
} */

/*
// Primary Java Virtual Machine
pub struct VM {
	priv threads : HashMap<uint, Chan<VMControlMessage> >,
	priv obj_broker_chan : SharedChan<ObjectBrokerMessage>,
}


impl VM {

	// ----------------------------------------------
	// Create a VM instance. To actually run code,
	// 
	pub fn create() -> MutexArc<VM> {
		MutexArc::new(VM {

		})
	}


	// ----------------------------------------------
	// Exit the VM. This interrupts all threads and
	// therefore forces them to terminate.
	pub fn exit(mut self) {

	}
} */



// A context of execution in the VM, typically associated with, but
// not necessarily limited to, interpreting a java.lang.Thread
// instance.
pub struct ThreadContext {
	// back reference to the VM, used to spawn off
	// further threads.

	// id of this thread. Unique across all threads as it
	// is drawn from an atomic counter.
	priv tid : uint,

	priv heap : LocalHeap,

	// connection to object broker
	priv broker_port : Port<ObjectBrokerMessage>,
	priv broker_chan : SharedChan<ObjectBrokerMessage>,

	priv opstack : ~[u32],
	priv locals : ~[u32],

	priv frames : ~[FrameInfo],
}

static mut ThreadContextIdCounter : uint = 0;

impl ThreadContext {

	// ----------------------------------------------
	pub fn new(/*vm : &mut VM*/broker_chan : SharedChan<ObjectBrokerMessage>) -> ThreadContext 
	{
		// generate an unique thread id
		let id = unsafe {
			atomic_add(&mut ThreadContextIdCounter, 1, AcqRel)
		};

		// register this thread with the object broker
		let (port, chan) = Chan::new();
		broker_chan.send(OB_REGISTER(id, chan));

		let mut t = ThreadContext {
			tid : id,

			heap : LocalHeap::dummy(),
			broker_port : port,
			broker_chan : broker_chan,

			opstack : ~[],
			locals : ~[],
			frames : ~[],
		};

		t.heap = unsafe { LocalHeap::new_with_owner(&mut t) };
		t
	}


	#[inline]
	pub fn get_tid(&self) -> uint {
		self.tid
	}


	// ----------------------------------------------
	// Handle incoming messages from ObjectBroker until a message
	// satifies the given predicate. Messages are processed before
	// the predicate is consulted. This method blocks until a message
	// is received that satifies the predicate.
	pub fn handle_messages_until(&self, func : |o : ObjectBrokerMessage| -> bool) {

	}


	// ----------------------------------------------
	// Sends a message to ObjectBroker, does not block.
	pub fn send_message(&self, msg : ObjectBrokerMessage) {

	}





	// ----------------------------------------------
	// Execute the context concurrently. This transfers ownership
	// of the context into a separate task and yields a communication
	// channel for other threads to interrupt.
	pub fn execute(mut self) {
		do spawn {
			let mut inner = self;
			loop {
				inner.op();
			}
		}
	}


	// IMPL

	// ----------------------------------------------
	#[inline]
	fn op(&mut self) {

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














