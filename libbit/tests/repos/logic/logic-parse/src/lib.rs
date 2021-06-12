pub mod ast;
mod parser;
mod symbol;

use ast::*;
pub use ast::{Atom, Var};
pub use symbol::Sym;

pub type ParseResult<T> = Result<T, String>;

fn parse<'a, T, E: std::fmt::Display>(
    src: &'a str,
    parser: impl FnOnce(&'a str) -> Result<T, E>,
) -> ParseResult<T> {
    parser(src).map_err(|err| err.to_string())
}

pub fn parse_program(src: &str) -> ParseResult<Program> {
    parse(src, |src| parser::ProgramParser::new().parse(src))
}

pub fn parse_clause(src: &str) -> ParseResult<Clause> {
    parse(src, |src| parser::ClauseParser::new().parse(src))
}

pub fn parse_goal(src: &str) -> ParseResult<Goal> {
    parse(src, |src| parser::GoalParser::new().parse(src))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_term_test() {
        let _term = parser::TermParser::new().parse("a(b,c,d)").unwrap();
        let _term = parser::TermParser::new().parse("cool(bob)").unwrap();
    }

    #[test]
    fn parse_clause_test() {
        let _fact = parser::ClauseParser::new().parse("cool(bob)").unwrap();
        let _fact = parser::ClauseParser::new().parse("cool(X)").unwrap();
        let _clause = parser::ClauseParser::new().parse("cool(bob) :- cool(frank)").unwrap();
        let _clause = parser::ClauseParser::new().parse("cool(bob) :- cool(f), cool(jen)").unwrap();
    }

    #[test]
    fn parse_forall_clause_test() {
        let _clause = parse_clause("forall<X,Y,Z> cool(X)").unwrap();
        println!("{}", _clause);
    }

    #[test]
    fn parse_program_test() {
        let _prog = parse_program("cool(jen). cool(bob). cool(X) :- cool(jen).").unwrap();
    }
}
