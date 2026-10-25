//! Logical monotonic arena model. It does not use compiler layout or emitters.
//! Offsets refer to payloads inside the charged storage (excluding owner state).
#[derive(Debug)]
pub struct Allocation {
    pub offset: usize,
    pub requested: usize,
    pub live: bool,
    pub data: Vec<u8>,
}

#[derive(Debug)]
pub struct Arena {
    pub bytes: usize,
    pub events: usize,
    pub used: usize,
    pub records: Vec<Allocation>,
}

impl Arena {
    pub fn new(bytes: usize, events: usize) -> Self {
        Self { bytes, events, used: 0, records: Vec::new() }
    }

    pub fn allocate(&mut self, requested: u64) -> Result<Option<usize>, ()> {
        if requested == 0 {
            return Ok(None);
        }
        let size = usize::try_from(requested).map_err(|_| ())?;
        let rounded = size.checked_add(15).ok_or(())? / 16 * 16;
        let end = self.used.checked_add(32).and_then(|v| v.checked_add(rounded)).ok_or(())?;
        if end > self.bytes || self.records.len() >= self.events {
            return Err(());
        }
        let offset = self.used + 32;
        self.records.push(Allocation { offset, requested: size, live: true, data: vec![0; rounded] });
        self.used = end;
        Ok(Some(offset))
    }

    fn index(&self, pointer: usize) -> Result<usize, ()> {
        self.records.iter().position(|record| record.offset == pointer && record.live).ok_or(())
    }

    pub fn release(&mut self, pointer: Option<usize>) -> Result<(), ()> {
        if let Some(pointer) = pointer {
            let index = self.index(pointer)?;
            self.records[index].live = false;
        }
        Ok(())
    }

    pub fn resize(&mut self, pointer: Option<usize>, requested: u64) -> Result<Option<usize>, ()> {
        let Some(pointer) = pointer else { return self.allocate(requested) };
        let index = self.index(pointer)?;
        if requested == 0 {
            self.records[index].live = false;
            return Ok(None);
        }
        let result = self.allocate(requested)?;
        let length = self.records[index].requested.min(usize::try_from(requested).unwrap());
        let prefix = self.records[index].data[..length].to_vec();
        self.records.last_mut().unwrap().data[..length].copy_from_slice(&prefix);
        self.records[index].live = false;
        Ok(result)
    }

    pub fn fill(&mut self, pointer: Option<usize>, byte: u8) -> usize {
        let index = self.index(pointer.unwrap()).unwrap();
        let record = &mut self.records[index];
        record.data[..record.requested].fill(byte);
        record.requested
    }
}

#[derive(Debug)]
pub struct Vector {
    pub pointer: Option<usize>,
    pub capacity: usize,
    pub values: Vec<Vec<u8>>,
    pub live: bool,
    width: usize,
    limit: usize,
}

impl Vector {
    pub fn new(width: usize, limit: usize) -> Self {
        Self { pointer: None, capacity: 0, values: Vec::new(), live: true, width, limit }
    }

    pub fn reserve(&mut self, arena: &mut Arena, minimum: u64) -> Result<(), ()> {
        if !self.live || minimum > self.limit as u64 {
            return Err(());
        }
        if minimum as usize <= self.capacity {
            return Ok(());
        }
        let capacity = (minimum as usize).max(self.capacity * 2).min(self.limit);
        let pointer = arena.resize(self.pointer, (capacity * self.width) as u64)?;
        self.pointer = pointer;
        self.capacity = capacity;
        Ok(())
    }

    pub fn push(&mut self, arena: &mut Arena, byte: u8) -> Result<(), ()> {
        self.reserve(arena, self.values.len() as u64 + 1)?;
        self.values.push(vec![byte; self.width]);
        self.sync(arena);
        Ok(())
    }

    pub fn get(&self, index: u64) -> Result<Vec<u8>, ()> {
        if !self.live {
            return Err(());
        }
        self.values.get(usize::try_from(index).map_err(|_| ())?).cloned().ok_or(())
    }

    pub fn remove(&mut self, arena: &mut Arena, index: u64) -> Result<(), ()> {
        self.get(index)?;
        self.values.remove(index as usize);
        self.sync(arena);
        Ok(())
    }

    pub fn set(&mut self, arena: &mut Arena, index: u64, byte: u8) -> Result<(), ()> {
        self.set_value(arena, index, vec![byte; self.width])
    }

    pub fn set_value(&mut self, arena: &mut Arena, index: u64, value: Vec<u8>) -> Result<(), ()> {
        assert_eq!(value.len(), self.width);
        self.get(index)?;
        self.values[index as usize] = value;
        self.sync(arena);
        Ok(())
    }

    pub fn insert(&mut self, arena: &mut Arena, index: u64, byte: u8) -> Result<(), ()> {
        self.insert_value(arena, index, vec![byte; self.width])
    }

    pub fn insert_value(&mut self, arena: &mut Arena, index: u64, value: Vec<u8>) -> Result<(), ()> {
        assert_eq!(value.len(), self.width);
        if index > self.values.len() as u64 {
            return Err(());
        }
        self.reserve(arena, self.values.len() as u64 + 1)?;
        self.values.insert(index as usize, value);
        self.sync(arena);
        Ok(())
    }

    pub fn drop(&mut self, arena: &mut Arena) -> Result<(), ()> {
        if !self.live {
            return Err(());
        }
        arena.release(self.pointer)?;
        self.pointer = None;
        self.capacity = 0;
        self.values.clear();
        self.live = false;
        Ok(())
    }

    fn sync(&self, arena: &mut Arena) {
        let index = arena.index(self.pointer.unwrap()).unwrap();
        let data = &mut arena.records[index].data;
        data.fill(0);
        for (i, value) in self.values.iter().enumerate() {
            data[i * self.width..(i + 1) * self.width].copy_from_slice(value);
        }
    }
}
