use std::rc::Rc;

use crate::{Interner, PrologTermData, Ty};

pub type UnificationResult<T> = Result<T, UnificationError>;
pub struct UnificationError;

pub struct UnificationContext<I: Interner> {
    interner: I,
}

impl<I: Interner> UnificationContext<I> {
    pub fn unify<T: Unify<I>>(&self, a: &T, b: &T) -> T {
        todo!()
    }
}

pub trait Unify<I: Interner> {
    fn unify(context: &mut I::UnificationContext, a: &Self, b: &Self) -> UnificationResult<()>;
}

impl<I: Interner, T: Unify<I>> Unify<I> for Rc<T> {
    fn unify(context: &mut I::UnificationContext, a: &Self, b: &Self) -> UnificationResult<()> {
        Self::unify(context, a, b)
    }
}

impl<'a, I: Interner, T: Unify<I>> Unify<I> for [T] {
    fn unify(context: &mut I::UnificationContext, xs: &Self, ys: &Self) -> UnificationResult<()> {
        xs.iter()
            .zip(ys.iter())
            .map(|(x, y)| Unify::unify(context, x, y))
            .collect::<UnificationResult<()>>()
    }
}

pub trait Unifier<I: Interner> {
    type Term: Unify<I>;
    fn unify(&mut self, x: &Self::Term, y: &Self::Term) -> UnificationResult<Self::Term>;
    fn interner(&self) -> I;
}

pub struct TyUnifier<I: Interner> {
    interner: I,
}

impl<I> Unifier<I> for TyUnifier<I>
where
    I: Interner,
{
    type Term = Ty<I>;

    fn interner(&self) -> I {
        self.interner
    }

    fn unify(&mut self, x: &Self::Term, y: &Self::Term) -> UnificationResult<Self::Term> {
        todo!()
    }
}

pub struct PrologUnifier<I: Interner> {
    interner: I,
}

impl<I> Unifier<I> for PrologUnifier<I>
where
    I: Interner,
{
    type Term = PrologTermData<I>;

    fn unify(&mut self, x: &Self::Term, y: &Self::Term) -> UnificationResult<Self::Term> {
        todo!()
    }

    fn interner(&self) -> I {
        self.interner
    }
}

pub trait Zipper<I: Interner> {}

pub trait Zip<I: Interner> {
    /// Uses the zipper to walk through two values, ensuring that they match.
    fn zip_with<Z: Zipper<I>>(zipper: &mut Z, a: &Self, b: &Self) -> Option<()>;
}
