
use std::hashmap::{HashMap};

use std::unstable::atomics::{atomic_add, AcqRel};

use std::ptr;

use vm::{ThreadContext};
use object::{JavaObject, JavaObjectId};
use class::{JavaClassRef};
use objectbroker::*;


// LocalHeap is a thread-local utility for threads to create,
// destroy and access Java objects. Even though it is technically
// not a heap (actual heap management is forwarded to Rust's
// runtime heap manager), it is referred to as such because of it
// behaviour which is to provide the Java heap.
//
// LocalHeap is tightly 1:1 coupled to a ThreadContext. 
// ThreadContext forwards all OB_RQ messages that it receives from 
// ObjectBroker to LocalHeap.


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

	fn get_thread_mut<'t>(&'t self) -> &'t mut ThreadContext {
		unsafe { &mut *self.thread }
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
		let op = OB_REMOTE_OBJECT_OP(self.tid, id,REMOTE_ADD_REF);

		self.get_thread_mut().send_message(op);
		self.owned_objects.insert(id, ~JavaObject::new(jclass, id));
		id
	}


	// ----------------------------------------------
	pub fn new_array_object() {
		// TODO
	}


	// ----------------------------------------------
	// AddRef a specific java object. This works both for local
	// objects (i.e. owned by current thread) and for remote
	// objects.
	pub fn add_ref(&mut self, oid : JavaObjectId) {
		// if this is a local object, addref it
		match self.owned_objects.find_mut(&oid) {
			Some(obj) => {
				obj.intern_add_ref();
				return
			},
			// fallthru
			None => () 
		}
		// forward request to ObjectBroker for remote objects
		let op = OB_REMOTE_OBJECT_OP(self.tid, oid, REMOTE_ADD_REF);
		self.get_thread().send_message(op);
	}


	// ----------------------------------------------
	// AddRef a specific java object. This works both for local
	// objects (i.e. owned by current thread) and for remote
	// objects.
	pub fn release(&mut self, oid : JavaObjectId) {
		// forward request to ObjectBroker for remote objects
		if !self.owned_objects.contains_key(&oid) {
			let op = OB_REMOTE_OBJECT_OP(self.tid, oid, REMOTE_RELEASE);
			self.get_thread().send_message(op);
			return;
		}

		{
			// if this is a local object, release it 
			let m = self.owned_objects.find_mut(&oid).unwrap();
			if m.intern_release() {
				return;
			}
		}
		
		// the object's reference counter reached zero
		// and we can therefore safely drop it.
		self.owned_objects.pop(&oid);
	}


	// ----------------------------------------------
	// Access a specific object. If the object requested is owned (or 
	// locked, depending on the access mode requested) by the current 
	// thread, access is immediately granted, otherwise the current 
	// task blocks until ownership can be obtained.
	//
	// The `access` parameter specifies the kind of access requested on 
	// the object. Note that OBJECT_ACCESS_NORMAL is always granted unless
	// the thread who currently owns that object is deadlocked and any
	// of the MONITOR_ access modes can be a cause of deadlock.
	//
	// TODO: how do we deal with deadlocks in general?
	//
	// The closure passed in is called exactly once with a borrowed ref to
	// the object, to which it gets full access but cannot dispose of
	pub fn access_object(&mut self, access : RequestObjectAccessType, 
		oid : JavaObjectId, wrap : |&JavaObject| -> ()) 
	{
		match self.owned_objects.find(&oid) {
			Some(ref mut obj) => {

				// TODO: implement access modes

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

		self.get_thread().send_message(OB_REMOTE_OBJECT_OP(self.tid, oid, REMOTE_OWN(access) ));
		self.get_thread_mut().handle_messages_until(|msg : &ObjectBrokerMessage| -> bool {
			match *msg {
				OB_REMOTE_OBJECT_OP(ref rtid, ref roid, REMOTE_DISOWN(ref obj, ref rec)) => {
					// when waiting for objects, we always block on
					// obtaining them so it is not possible that 
					// multiple requests are sent and responses
					// received in a different order.
					assert_eq!(*rec, self.tid);

					// also verify that the access mode requirement is fullfilled
					/*
					assert!(access != OBJECT_ACCESS_Monitor || 
						    access != OBJECT_ACCESS_MonitorPriority || 
						    obj.monitor().can_enter(self.tid)
					); */
					true
				},
				_ => false
			}
		});

		self.access_object(access, oid, wrap)
	}


	// ----------------------------------------------
	// Transfer ownership of an object to a particular thread
	pub fn send_to_thread(&mut self, oid : JavaObjectId, tid : uint) {
		let obj = self.owned_objects.pop(&oid).unwrap();
		let m = OB_REMOTE_OBJECT_OP(self.tid, oid, REMOTE_DISOWN(obj, tid));
		self.get_thread().send_message(m);
	}


	// ----------------------------------------------
	// Check if a particular object is currently owned by this thread
	pub fn owns(&self, b : JavaObjectId) -> bool {
		return self.owned_objects.contains_key(&b);
	}


	// ----------------------------------------------
	// Handle any of the remote object messages 
	// a is the source thread id, and b is the object in question.
	pub fn handle_message(&mut self, a : uint, b : JavaObjectId, op : RemoteObjectOpMessage) {
		assert!(self.owns(b));
		match op {
			REMOTE_WHO_OWNS => fail!("logic error, WHO_OWNS is not handled by threads"),
			REMOTE_ADD_REF => self.add_ref(b),
			REMOTE_RELEASE => self.release(b),
			REMOTE_OWN(mode) => {
				// TODO: handle request modes and monitor access
				self.send_to_thread(b, a);
			},
			
			REMOTE_DISOWN(obj,rec) => {
				// currently we should not be receiving objects that we
				// did not request using OB_RQ_OWN
				assert_eq!(rec, self.tid);
				self.owned_objects.insert(b, obj);
			},
		}
	}
}
