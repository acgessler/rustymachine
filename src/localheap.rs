
use std::hashmap::{HashMap};

use std::unstable::atomics::{atomic_add, AcqRel};

use std::ptr;

use vm::{ThreadContext};
use object::{JavaObject, JavaObjectId};
use class::{JavaClassRef};
use objectbroker::{ObjectBrokerMessage, OB_RQ_DISOWN, OB_RQ_OWN, OB_RQ_ADD_REF, OB_RQ_RELEASE};

// LocalHeap is a thread-local utility for threads to create,
// destroy and access Java objects. Even though it is technically
// not a heap (actual heap management is forwarded to Rust's
// runtime heap manager), it is referred to as such because of it
// behaviour which is to provide the Java heap. LocalHeap is tightly 
// coupled to a ThreadContext. For cross-thread access to objects, 
// LocalHeap holds a connection to the VM's ObjectBroker.

pub struct LocalHeap {
	// backref to owning thread. Unfortunately a borrowed ref
	// cannot solve this so we need an unsafe pointer.
	// http://stackoverflow.com/questions/20698384
	priv thread : *mut ThreadContext,

	// shortcut to thread-id
	priv tid : uint,
	
	// heap objects currently owned by this thread context
	priv owned_objects : HashMap<JavaObjectId, ~JavaObject>,

	// 
}

static LH_INITIAL_OBJ_HASHMAP_CAPACITY : uint = 1024;
static mut ObjectIdCounter : JavaObjectId = 0;

impl LocalHeap  {

	// ----------------------------------------------
	pub fn dummy() -> LocalHeap {
		LocalHeap {
			thread : ptr::mut_null(),
			tid : 0,
			owned_objects : HashMap::new(),
		}
	}


	// ----------------------------------------------
	pub unsafe fn new_with_owner(t : &mut ThreadContext) -> LocalHeap {
		LocalHeap {
			thread : ptr::to_mut_unsafe_ptr(t),
			tid : t.get_tid(),

			owned_objects : HashMap::with_capacity(LH_INITIAL_OBJ_HASHMAP_CAPACITY),
		}
	}


	// ----------------------------------------------
	fn get_thread<'t>(&'t self) -> &'t ThreadContext {
		unsafe { &*self.thread }
	}


	// ----------------------------------------------
	pub fn new_object(&mut self, jclass : JavaClassRef) -> JavaObjectId {
		// generate an unique object id
		let id = unsafe {
			atomic_add(&mut ObjectIdCounter, 1, AcqRel)
		};

		// this id must be unique - if not, we ran out of
		// 64bit indices ("impossible - our shields cannot be 
		// broken") or there is a logic flaw somewhere.
		assert!(!self.owned_objects.contains_key(&id));

		// tell the object broker to ensure other threads
		// can request the object by its oid
		self.get_thread().send_message(OB_RQ_ADD_REF(self.tid, id));
		self.owned_objects.insert(id, ~JavaObject::new(jclass, id));
		id
	}


	// ----------------------------------------------
	pub fn new_array_object() {
		// TODO
	}


	// ----------------------------------------------
	// Access a specific object. If the object requested
	// is owned by the current thread, access is immediately
	// granted, otherwise the current task blocks until 
	// ownership can be obtained.
	pub fn access_object(&mut self, oid : JavaObjectId, wrap : |&JavaObject| -> ()) {
		match self.owned_objects.find(&oid) {
			Some(ref mut obj) => {
				wrap(**obj);
						
				// check if we have any pending requests for this object,
				// if so, satisfy them immediately 
			/*	match obj.pop_waiting_thread() {
					None => (),
					Some(tid) => {
						self.send_to_thread(obj, tid);
					}
				} */
				return
			},
			// fallthru
			None => () 
		}

		{
			let ref thread = self.get_thread();
			thread.send_message(OB_RQ_OWN(self.tid, oid));
			thread.handle_messages_until(|msg : ObjectBrokerMessage| -> bool {
				match msg {
					OB_RQ_DISOWN(rtid, roid, obj, rec) => {
						// when waiting for objects, we always block on
						// obtaining them so it is not possible that 
						// multiple requests are sent and responses
						// received in a different order.
						assert_eq!(rec, self.tid);
						true
					},
					_ => false
				}
			});
		}

		self.access_object(oid, wrap)
	}


	// ----------------------------------------------
	// Transfer ownership of an object to a particular thread
	pub fn send_to_thread(&self, obj : ~JavaObject, tid : uint) {
		let m = OB_RQ_DISOWN(self.tid, obj.get_oid(), obj, tid);
		self.get_thread().send_message(m);
	}
}