
use std::unstable::atomics::{atomic_add, AcqRel};

use std::hashmap::{HashMap};

use objectbroker::*;

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


	// ----------------------------------------------
	// Get the unique thread-id of the thread
	#[inline]
	pub fn get_tid(&self) -> uint {
		self.tid
	}


	// ----------------------------------------------
	// Handle incoming messages from ObjectBroker until a message
	// satifies the given predicate. Messages are processed after
	// the predicate is consulted, but the message for which the
	// predicate returns true is still processed. 
	//
	// This method blocks until a message is received that satifies 
	// the predicate.
	pub fn handle_messages_until(&mut self, pred : |o : &ObjectBrokerMessage| -> bool) {
		loop {
			let msg = self.broker_port.recv();
			let b = pred(&msg);

			self.handle_message(msg);
			if b {
				break;
			}
		}
	}


	// ----------------------------------------------
	// Sends a message to another thread via ObjectBroker, does 
	// not block.
	pub fn send_message(&self, msg : ObjectBrokerMessage) {
		self.broker_chan.send(msg);
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
	fn handle_message(&mut self, o : ObjectBrokerMessage) {
		match o {
			// those are not supported in this messaging direction
			// (i.e. they are only _sent_ to ObjectBroker)
			OB_REGISTER(a,b) => fail!("REGISTER message not expected here"),

			OB_SHUTDOWN => fail!("todo"),
			
			// TODO: handle thread interrupt

			OB_REMOTE_OBJECT_OP(a,b,op) => 
				self.heap.handle_message(a,b,op),
		}
	}


	// ----------------------------------------------
	#[inline]
	fn op(&mut self) {

	}
}














