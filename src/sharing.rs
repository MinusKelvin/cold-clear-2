use parking_lot::{Condvar, Mutex, RwLock};

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

    pub fn start(&self, data: T) {
        let access_control = self.access_control.lock();

        *self.data.write() = Some(data);
        self.create_event.notify_all();

        drop(access_control);
    }

    pub fn stop(&self) {
        let access_control = self.access_control.lock();

        *self.data.write() = None;

        drop(access_control);
    }

    pub fn write_op<R>(&self, op: impl FnOnce(&mut T) -> R) -> R {
        let mut access_guard = self.access_control.lock();
        loop {
            let mut write_guard = self.data.write();
            match write_guard.as_mut() {
                None => {
                    drop(write_guard);
                    self.create_event.wait(&mut access_guard);
                }
                Some(state) => {
                    drop(access_guard);
                    return op(state);
                }
            }
        }
    }

    pub fn write_op_if_exists<R>(&self, op: impl FnOnce(&mut T) -> R) -> Option<R> {
        let access_guard = self.access_control.lock();
        let mut write_guard = self.data.write();
        drop(access_guard);
        write_guard.as_mut().map(op)
    }

    pub fn read_op<R>(&self, op: impl FnOnce(&T) -> R) -> R {
        let mut access_guard = self.access_control.lock();
        loop {
            let read_guard = self.data.read();
            match read_guard.as_ref() {
                None => {
                    drop(read_guard);
                    self.create_event.wait(&mut access_guard);
                }
                Some(state) => {
                    drop(access_guard);
                    return op(state);
                }
            }
        }
    }

    pub fn read_op_if_exists<R>(&self, op: impl FnOnce(&T) -> R) -> Option<R> {
        let access_guard = self.access_control.lock();
        let read_guard = self.data.read();
        drop(access_guard);
        read_guard.as_ref().map(op)
    }
}
