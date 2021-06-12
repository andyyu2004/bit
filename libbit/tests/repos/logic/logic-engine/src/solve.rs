// use crate::db::Database;
// use crate::unify::InferCtxt;
use logic_ir::*;

// pub trait Solver<I: Interner> {
// fn solve(&mut self, db: &Database, env: Environment<I>, goal: &GoalData<I>) -> Solution<I> {
// todo!()
// }
// }

// pub struct RecursiveSolver<I: Interner> {
// pub interner: I,
// pub env: Environment<I>,
// pub infcx: InferCtxt<I>,
// }

// impl<I: Interner> RecursiveSolver<I> {
// pub fn new(interner: I, env: Environment<I>) -> Self {
// Self { interner, env, infcx: InferCtxt::new(interner) }
// }

// pub fn solve(&self, goal: &Goal<I>) -> Option<Solution<I>> {
// match self.interner.goal_data(goal) {
// GoalData::DomainGoal(term) => {
// for clause in self.interner.clauses(&self.env.clauses) {
// let clause = self.interner.clause_data(clause);
// match clause {
// ClauseData::Horn(consequent, conditions) => {
// if let Some(term) = self.infcx.try_unify(term, consequent) {
// dbg!("unifiable");
// for condition in self.interner.goals(conditions) {
// if self.solve(condition).is_none() {
// continue;
// }
// }
// return Some(Solution::Term(term));
// }
// }
// };
// }
// return None;
// }
// GoalData::And(..) | GoalData::Or(..) => todo!(),
// }
// }
// }

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Environment<I: Interner> {
    clauses: Clauses<I>,
}

impl<I: Interner> Environment<I> {
    pub fn new(clauses: Clauses<I>) -> Self {
        Self { clauses }
    }
}

// #[derive(Debug)]
// pub enum Solution<I: Interner> {
// TODO
// Unique(Substs<I>),
// Term(GenericTerm<I>),
// }
