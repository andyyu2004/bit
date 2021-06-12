pub trait Unify {
    fn unify(&self, unifier: impl Unifier, other: &Self) -> Self;
}

pub trait Unifier<I: Interner> {}
