use crate::*;
use logic_error::LogicResult;
use logic_ir::{IRInterner, Program};
use logic_parse::{ast, ParseResult};
use std::sync::Arc;

trait IRDatabase<I> {
    fn interner(&self) -> I;
}

#[salsa::query_group(Logic)]
pub trait LogicDatabase: salsa::Database {}

#[salsa::query_group(Lowering)]
pub trait LoweringDatabase: salsa::Database {
    #[salsa::input]
    fn src(&self) -> Arc<String>;
    fn interner(&self) -> IRInterner;
    fn ast(&self) -> ParseResult<ast::Program>;
    fn ir(&self) -> ParseResult<Program<IRInterner>>;
    fn env(&self) -> LogicResult<Environment<IRInterner>>;
}

#[salsa::database(Lowering, Logic)]
#[derive(Default)]
pub struct Database {
    storage: salsa::Storage<Self>,
}

impl salsa::Database for Database {
}

impl Database {
    pub fn new(src: String) -> Self {
        let mut db = Self::default();
        db.set_src(Arc::new(src));
        db
    }
}

fn ast(db: &dyn LoweringDatabase) -> ParseResult<ast::Program> {
    let src = db.src();
    logic_parse::parse_program(&src)
}

fn ir(db: &dyn LoweringDatabase) -> ParseResult<Program<IRInterner>> {
    let ast = db.ast()?;
    Ok(logic_ir::lower_ast(&ast))
}

fn interner(db: &dyn LoweringDatabase) -> IRInterner {
    IRInterner
}

fn env(db: &dyn LoweringDatabase) -> LogicResult<Environment<IRInterner>> {
    let ir = db.ir()?;
    let env = Environment::new(ir.clauses);
    Ok(env)
}
