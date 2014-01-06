

// Core VM API and thread management
// (but not actual bytecode interpretation - see thread.rs for this)

use extra::arc::{RWArc};
use std::hashmap::{HashMap};

use objectbroker::*;
use classloader::{ClassLoader};
use object::{JavaObjectId};


/*
// Messages exchanged between the VM and individual ThreadContext's
pub enum VMControlMessage {
	// interrupt a specific thread
	VM_CONTROL_INTERRUPT(id),

	// System.exit(id)
	VM_CONTROL_EXIT(id),
} */


struct VMData {
	//priv classloader : ClassLoader,
	priv threads : HashMap<uint, bool >,
	//priv obj_broker_chan : SharedChan<ObjectBrokerMessage>,
}


// Primary Java Virtual Machine API
pub struct VM {
	priv inner : RWArc<VMData>,
}


impl VM {

	// ----------------------------------------------
	// 
	// 
	pub fn new(classloader : ClassLoader) -> VM {
		let broker = ObjectBroker::new();
		VM {
			inner : RWArc::new(VMData {
				//classloader : classloader,
				threads : HashMap::new(),
				//obj_broker_chan : broker.launch(),
			})
		}
	}


	// ----------------------------------------------
	// 
	// 
	pub fn run_thread(class : &str, method : &str, obj : Option<JavaObjectId>) {
		// TODO
	}


	// ----------------------------------------------
	// Exit the VM. This interrupts all threads and
	// therefore forces them to terminate.
	//
	// This is a synchronous API.
	pub fn exit(mut self) {
		// TODO
	}
} 














