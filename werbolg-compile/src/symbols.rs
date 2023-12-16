use alloc::vec::Vec;
use core::hash::Hash;
use core::marker::PhantomData;
use hashbrown::HashMap;
use werbolg_core::id::{Id, IdRemapper};
use werbolg_core::Ident;

pub struct SymbolsTable<ID: IdRemapper> {
    pub(crate) tbl: HashMap<Ident, Id>,
    phantom: PhantomData<ID>,
}

impl<ID: IdRemapper> SymbolsTable<ID> {
    pub fn new() -> Self {
        Self {
            tbl: Default::default(),
            phantom: PhantomData,
        }
    }

    pub fn insert(&mut self, ident: Ident, id: ID) {
        self.tbl.insert(ident, id.uncat());
    }

    pub fn get(&self, ident: &Ident) -> Option<ID> {
        self.tbl.get(ident).map(|i| ID::cat(*i))
    }

    pub fn iter(&self) -> impl Iterator<Item = (&Ident, ID)> {
        self.tbl.iter().map(|(ident, id)| (ident, ID::cat(*id)))
    }
}

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

    pub fn concat(&mut self, after: &mut IdVecAfter<ID, T>) {
        assert!(self.vec.len() == after.ofs.uncat().as_index());
        self.vec.append(&mut after.id_vec.vec)
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

pub struct IdVecAfter<ID, T> {
    id_vec: IdVec<ID, T>,
    ofs: ID,
}

impl<ID: IdRemapper, T> IdVecAfter<ID, T> {
    pub fn new(first_id: ID) -> Self {
        Self {
            id_vec: IdVec::new(),
            ofs: first_id,
        }
    }

    pub fn from_idvec(id_vec: IdVec<ID, T>, first_id: ID) -> Self {
        Self {
            id_vec,
            ofs: first_id,
        }
    }

    pub fn push(&mut self, v: T) -> ID {
        let id = self.id_vec.push(v).uncat();
        let new_id = Id::remap(id, self.ofs.uncat());
        ID::cat(new_id)
    }

    pub fn remap<F>(&mut self, f: F)
    where
        F: Fn(&mut T) -> (),
    {
        for elem in self.id_vec.iter_mut() {
            f(elem)
        }
    }
}

pub struct SymbolsTableData<ID: IdRemapper, T> {
    pub table: SymbolsTable<ID>,
    pub vecdata: IdVec<ID, T>,
}

impl<ID: IdRemapper, T> SymbolsTableData<ID, T> {
    pub fn new() -> Self {
        Self {
            table: SymbolsTable::new(),
            vecdata: IdVec::new(),
        }
    }

    pub fn add(&mut self, ident: Ident, v: T) -> Option<ID> {
        if self.table.get(&ident).is_some() {
            return None;
        }
        let id = self.vecdata.push(v);
        self.table.insert(ident, id);
        Some(id)
    }

    pub fn add_anon(&mut self, v: T) -> ID {
        self.vecdata.push(v)
    }
}

pub struct UniqueTableBuilder<ID: IdRemapper, T: Eq + Hash> {
    pub symtbl: HashMap<T, ID>,
    pub syms: IdVec<ID, T>,
    pub phantom: PhantomData<ID>,
}

impl<ID: IdRemapper, T: Clone + Eq + Hash> UniqueTableBuilder<ID, T> {
    pub fn new() -> Self {
        Self {
            symtbl: HashMap::new(),
            syms: IdVec::new(),
            phantom: PhantomData,
        }
    }

    pub fn add(&mut self, data: T) -> ID {
        if let Some(id) = self.symtbl.get(&data) {
            *id
        } else {
            let id = self.syms.push(data.clone());
            self.symtbl.insert(data, id);
            id
        }
    }

    pub fn finalize(self) -> IdVec<ID, T> {
        self.syms
    }
}
