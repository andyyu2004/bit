// use indexed_vec::Idx;
use logic_ir::*;
use std::cell::RefCell;
use std::fmt::{self, Debug, Formatter};

// pub enum UnificationError {
// // TODO
// Failed,
// }

// #[derive(Clone, Copy, PartialEq, Eq, Hash)]
// pub struct InferVar<I: Interner> {
// idx: InferenceIdx,
// phantom: std::marker::PhantomData<I>,
// }

// impl<I: Interner> InferVar<I> {
// pub fn new(idx: InferenceIdx) -> Self {
// Self { idx, phantom: std::marker::PhantomData }
// }
// }

// #[derive(Debug, PartialEq, Eq, Clone)]
// pub enum InferenceValue<I: Interner> {
// Bound(GenericTerm<I>),
// Unbound,
// }

// impl<I: Interner> Debug for InferVar<I> {
// fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
// write!(f, "?{:?}", self.idx)
// }
// }

// impl<I: Interner> ena::unify::UnifyValue for InferenceValue<I> {
// type Error = ena::unify::NoError;

// /// Given two values, produce a new value that combines them.
// /// If that is not possible, produce an error.
// fn unify_values(x: &Self, y: &Self) -> Result<Self, Self::Error> {
// Ok(match (x, y) {
// (Self::Bound(_), Self::Bound(_)) => panic!("unifying two known values"),
// (Self::Bound(_), Self::Unbound) => x.clone(),
// (Self::Unbound, Self::Bound(_)) => y.clone(),
// (Self::Unbound, Self::Unbound) => Self::Unbound,
// })
// }
// }

// impl<I: Interner> ena::unify::UnifyKey for InferVar<I> {
// type Value = InferenceValue<I>;

// fn index(&self) -> u32 {
// self.idx.index() as u32
// }

// fn from_index(idx: u32) -> Self {
// Self::new(InferenceIdx::new(0))
// }

// fn tag() -> &'static str {
// "InferenceVar"
// }
// }

// pub struct InferCtxt<I: Interner> {
// interner: I,
// inner: RefCell<InferCtxtInner<I>>,
// }

// pub struct InferCtxtInner<I: Interner> {
// tables: ena::unify::InPlaceUnificationTable<InferVar<I>>,
// vars: Vec<InferVar<I>>,
// }

// impl<I: Interner> Default for InferCtxtInner<I> {
// fn default() -> Self {
// Self { tables: Default::default(), vars: Default::default() }
// }
// }

// impl<I: Interner> InferCtxt<I> {
// pub fn new(interner: I) -> Self {
// Self { interner, inner: Default::default() }
// }

// pub fn try_unify(&self, t: &I::DomainGoal, u: &I::DomainGoal) -> Option<I::DomainGoal> {
// self.unify(t, u).ok()
// }

// pub fn unify(
// &self,
// t: &I::DomainGoal,
// u: &I::DomainGoal,
// ) -> Result<I::DomainGoal, UnificationError> {
// // maybe move this check somewhere else outside
// if t == u {
// return Ok(t.clone());
// }

// let (t, u) = (self.interner.term_data(t), self.interner.term_data(u));
// match (t, u) {
// (TermData::Var(..), _) | (_, TermData::Var(..)) =>
// unreachable!("vars should be instantiated? maybe?"),
// (&TermData::Infer(x), t) | (t, &TermData::Infer(x)) => {
// let term = GenericTerm::intern(self.interner, t.clone());
// self.instantiate(x, term.clone());
// Ok(term)
// }
// (TermData::Structure(f, xs), TermData::Structure(g, ys)) if f == g =>
// Ok(GenericTerm::intern(
// self.interner,
// TermData::Structure(*f, self.unify_iter(xs, ys)?),
// )),
// _ => Err(UnificationError::Failed),
// }
// }

// fn unify_iter(&self, ts: &Terms<I>, us: &Terms<I>) -> Result<Terms<I>, UnificationError> {
// let interner = self.interner;
// Ok(Terms::intern(
// interner,
// interner
// .terms(ts)
// .iter()
// .zip(interner.terms(us))
// .map(|(t, u)| self.unify(t, u))
// .collect::<Result<Vec<_>, _>>()?,
// ))
// }

// fn instantiate(&self, idx: InferenceIdx, t: GenericTerm<I>) {
// self.inner.borrow_mut().tables.union_value(InferVar::new(idx), InferenceValue::Bound(t));
// }
// }
