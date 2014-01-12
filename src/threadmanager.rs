// rustyVM - Java VM written in pure Rust
// Copyright (c) 2013 Alexander Gessler
//
// Permission is hereby granted, free of charge, to any person obtaining a copy of
// this software and associated documentation files (the "Software"), to deal in
// the Software without restriction, including without limitation the rights to
// use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of
// the Software, and to permit persons to whom the Software is furnished to do so,
// subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS
// FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR
// COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER
// IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN
// CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
//


use std::hashmap::{HashMap};

use objectbroker::{ObjectBroker};

// Global thread state and management. All threads maintain some global state
// in the central Broker messaging task. Global state includes scheduling info,
// join()-waitlists as well as whether threads are daemons or not.

// Notably, this design makes access to threads scale badly as it is inherently
// single-threaded. On the other side, it makes thread maintenance very easy
// and safe. It is reasonable to assume that most code will not too often interact 
// directly with java.lang.Thread.

// References:
// http://docs.oracle.com/javase/7/docs/api/java/lang/Thread.html
// http://docs.oracle.com/javase/7/docs/api/java/lang/ThreadGroup.html


// Remote messages responded to by the threadmanager (messages are received
// and forwarded by broker)
pub enum RemoteThreadOpMessage {

	THREAD_JOIN,
	THREAD_NOTIFY_TERMINATION,
	THREAD_SET_PRIORITY(int),
	THREAD_SET_NAME(~str)
}


pub struct GlobThreadInfo {
	tid : uint,

	// given name of the thread, used for debugging
	name : ~str,

	// java thread priority
	priority : int,

	//
	daemon : bool,
}


pub struct ThreadManager {

	priv threads : HashMap<uint, GlobThreadInfo>,

	// number of threads in `threads` with daemon=false,
	// when this counter reaches 0, the VM shuts down
	priv alive_nondaemon_count : uint,

	priv state : ThreadManagerState,

	// stopped threads get moved here so their parameters are still
	// available. TODO: how to prevent this from growing indefinitely
	priv stopped_threads : ~[GlobThreadInfo],
}


#[deriving(Eq)]
pub enum ThreadManagerState {
	// initial state when no thread has been added yet
	TMS_NoThreadSeenYet,

	// running state - at least one non-daemon thread
	TMS_Running,

	// all non-daemon threads have died. Transition from
	// here to Running is possible by adding a new thread
	// that is not a daemon.
	TMS_AllNonDaemonsDead,
}


impl ThreadManager {

	// ----------------------------------------------
	pub fn new() -> ThreadManager {
		ThreadManager{
			threads : HashMap::new(),

			alive_nondaemon_count : 0,
			state : TMS_NoThreadSeenYet,

			stopped_threads : ~[],
		}
	}


	// ----------------------------------------------
	pub fn get_state(&self) -> ThreadManagerState {
		self.state
	}


	// ----------------------------------------------
	// Register a thread with the ThreadManager
	pub fn add_thread(&mut self, tid : uint) {
		assert!(!self.threads.contains_key(&tid));

		self.threads.insert(tid, GlobThreadInfo {
			tid : tid,
			name : ~"",
			priority : 0,
			daemon : false,
		});

		self.alive_nondaemon_count += 1;
		self.state = TMS_Running;
	}


	// ----------------------------------------------
	// Unregister a thread from the ThreadManager. The thread
	// is shelved of to the so called 'stopped-threads' list,
	// which makes its name, priority and other parameter 
	// available even after it has been removed from the live
	// thread state.
	pub fn remove_thread(&mut self, tid : uint) {
		assert!(self.threads.contains_key(&tid));
		let t = self.threads.pop(&tid).unwrap();

		if !t.daemon {
			self.alive_nondaemon_count -= 1;
		}

		self.stopped_threads.push(t);
		self.state = if self.alive_nondaemon_count == 0 { TMS_AllNonDaemonsDead } else { TMS_Running };
	}


	// ----------------------------------------------
	// Change the 'daemon' flag of a given thread with immediate
	// effect. If this causes the last alive non-daemon thread to
	// become daemon, the thread manager's state changes to 
	// TMS_AllNonDaemonsDead
	pub fn set_daemon(&mut self, tid : uint, daemonize : bool) {
		assert!(self.threads.contains_key(&tid));

		let old = self.threads.get(&tid).daemon;
		if old == daemonize {
			return;
		}
		self.threads.get_mut(&tid).daemon = daemonize;

		assert!(!daemonize || self.alive_nondaemon_count > 0);
		self.alive_nondaemon_count += if daemonize { -1 } else { 1 };
		self.state = if self.alive_nondaemon_count == 0 { TMS_AllNonDaemonsDead } else { TMS_Running };
	}


	// ----------------------------------------------
	pub fn process_message(&mut self, src_tid : uint, dest_tid : uint, 
		op : RemoteThreadOpMessage)  {

		match op {
			THREAD_JOIN => (),
			THREAD_NOTIFY_TERMINATION => fail!("THREAD_NOTIFY_TERMINATION unexpected"),
			THREAD_SET_PRIORITY(prio) => (),
			THREAD_SET_NAME(name) => (),
		}
	}
}


#[cfg(test)]
mod tests {
	use threadmanager::*;

	#[test]
	fn test_threadmanager_lifecycle() {
		let mut t = ThreadManager::new();
		assert_eq!(t.get_state(), TMS_NoThreadSeenYet);

		t.add_thread(12);
		assert_eq!(t.get_state(), TMS_Running);
		t.add_thread(13);

		t.remove_thread(13);
		assert_eq!(t.get_state(), TMS_Running);

		t.remove_thread(12);
		assert_eq!(t.get_state(), TMS_AllNonDaemonsDead);

		t.add_thread(16);
		assert_eq!(t.get_state(), TMS_Running);
	}


	
	#[test]
	#[should_fail]
	fn test_threadmanager_groups_cannot_remove_nonexistent_thread() {
		let mut t = ThreadManager::new();
		t.remove_thread(1);
	}


	#[test]
	#[should_fail]
	fn test_threadmanager_groups_cannot_reuse_tid() {
		let mut t = ThreadManager::new();

		t.add_thread(12);
		t.add_thread(13);
		t.remove_thread(12);
		t.add_thread(12);
	}


	#[test]
	fn test_threadmanager_lifecycle_with_daemons() {
		let mut t = ThreadManager::new();
		t.add_thread(12);
		t.add_thread(13);

		t.set_daemon(13, true);
		assert_eq!(t.get_state(), TMS_Running);

		t.remove_thread(13);
		assert_eq!(t.get_state(), TMS_Running);

		t.set_daemon(12, true);
		assert_eq!(t.get_state(), TMS_AllNonDaemonsDead);
	}

	// TODO: test stopped-tid
}

