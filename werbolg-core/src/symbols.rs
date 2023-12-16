use super::id::{Id, IdRemapper};
use alloc::vec::Vec;
use core::marker::PhantomData;

pub struct IdVec<ID, T> {
    vec: Vec<T>,
    phantom: PhantomData<ID>,
}

impl<ID: IdRemapper, T> core::ops::Index<ID> for IdVec<ID, T> {
    type Output = T;

    fn index(&self, index: ID) -> &Self::Output {
        &self.vec[index.uncat().as_index()]
    }
}

impl<ID: IdRemapper, T> core::ops::IndexMut<ID> for IdVec<ID, T> {
    fn index_mut(&mut self, index: ID) -> &mut T {
        &mut self.vec[index.uncat().as_index()]
    }
}

impl<ID: IdRemapper, T> IdVec<ID, T> {
    pub fn new() -> Self {
        Self {
            vec: Vec::new(),
            phantom: PhantomData,
        }
    }

    pub fn get(&self, id: ID) -> Option<&T> {
        let idx = id.uncat().as_index();
        if self.vec.len() > idx {
            Some(&self.vec[idx])
        } else {
            None
        }
    }

    pub fn next_id(&self) -> ID {
        ID::cat(Id::from_slice_len(&self.vec))
    }

    pub fn push(&mut self, v: T) -> ID {
        let id = Id::from_slice_len(&self.vec);
        self.vec.push(v);
        ID::cat(id)
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut T> {
        self.vec.iter_mut()
    }

    pub fn iter(&self) -> impl Iterator<Item = (ID, &T)> {
        self.vec
            .iter()
            .enumerate()
            .map(|(i, t)| (ID::cat(Id::from_collection_len(i)), t))
    }

    pub fn into_iter(self) -> impl Iterator<Item = (ID, T)> {
        self.vec
            .into_iter()
            .enumerate()
            .map(|(i, t)| (ID::cat(Id::from_collection_len(i)), t))
    }

    pub fn remap<F, U>(self, f: F) -> IdVec<ID, U>
    where
        F: Fn(T) -> U,
    {
        let mut new = IdVec::<ID, U>::new();
        for (id, t) in self.into_iter() {
            let new_id = new.push(f(t));
            assert_eq!(new_id.uncat(), id.uncat());
        }
        new
    }
}
