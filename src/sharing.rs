use std::sync::{Condvar, Mutex, RwLock};

/// Writer-preferred Read/Write lock that can be empty, and can be blocked on until it is filled.
pub struct SharedState<T> {
    access_control: Mutex<()>,
    create_event: Condvar,
    data: RwLock<Option<T>>,
}

impl<T> SharedState<T> {
    pub fn new() -> Self {
        SharedState {
            access_control: Mutex::new(()),
            create_event: Condvar::new(),
            data: RwLock::new(None),
        }
    }

    pub fn write_op<R>(&self, op: impl FnOnce(&mut T) -> R) -> R {
        let mut access_guard = self.access_control.lock().unwrap();
        loop {
            let mut write_guard = self.data.write().unwrap();
            match write_guard.as_mut() {
                None => {
                    drop(write_guard);
                    access_guard = self.create_event.wait(access_guard).unwrap();
                }
                Some(state) => {
                    drop(access_guard);
                    return op(state);
                }
            }
        }
    }

    pub fn write_op_if_exists<R>(&self, op: impl FnOnce(&mut T) -> R) -> Option<R> {
        let access_guard = self.access_control.lock().unwrap();
        let mut write_guard = self.data.write().unwrap();
        drop(access_guard);
        write_guard.as_mut().map(op)
    }

    pub fn read_op<R>(&self, op: impl FnOnce(&T) -> R) -> R {
        let mut access_guard = self.access_control.lock().unwrap();
        loop {
            let read_guard = self.data.read().unwrap();
            match read_guard.as_ref() {
                None => {
                    drop(read_guard);
                    access_guard = self.create_event.wait(access_guard).unwrap();
                }
                Some(state) => {
                    drop(access_guard);
                    return op(state);
                }
            }
        }
    }

    pub fn read_op_if_exists<R>(&self, op: impl FnOnce(&T) -> R) -> Option<R> {
        let access_guard = self.access_control.lock().unwrap();
        let read_guard = self.data.read().unwrap();
        drop(access_guard);
        read_guard.as_ref().map(op)
    }

    pub fn stop(&self) {
        let access_control = self.access_control.lock().unwrap();

        *self.data.write().unwrap() = None;

        drop(access_control);
    }

    pub fn start(&self, data: T) {
        let access_control = self.access_control.lock().unwrap();

        *self.data.write().unwrap() = Some(data);
        self.create_event.notify_all();

        drop(access_control);
    }
}
