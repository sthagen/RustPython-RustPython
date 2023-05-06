use rustpython_codegen::{compile, symboltable};
use rustpython_parser::ast::{fold::Fold, ConstantOptimizer};

pub use rustpython_codegen::compile::CompileOpts;
pub use rustpython_compiler_core::{CodeObject, Mode};

// these modules are out of repository. re-exporting them here for convenience.
pub use rustpython_codegen as codegen;
pub use rustpython_compiler_core as core;
pub use rustpython_parser as parser;

use std::error::Error as StdError;
use std::fmt;

#[derive(Debug)]
pub enum CompileErrorType {
    Codegen(rustpython_codegen::error::CodegenErrorType),
    Parse(parser::ParseErrorType),
}

impl StdError for CompileErrorType {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            CompileErrorType::Codegen(e) => e.source(),
            CompileErrorType::Parse(e) => e.source(),
        }
    }
}
impl fmt::Display for CompileErrorType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            CompileErrorType::Codegen(e) => e.fmt(f),
            CompileErrorType::Parse(e) => e.fmt(f),
        }
    }
}
impl From<rustpython_codegen::error::CodegenErrorType> for CompileErrorType {
    fn from(source: rustpython_codegen::error::CodegenErrorType) -> Self {
        CompileErrorType::Codegen(source)
    }
}
impl From<parser::ParseErrorType> for CompileErrorType {
    fn from(source: parser::ParseErrorType) -> Self {
        CompileErrorType::Parse(source)
    }
}

pub type CompileError = rustpython_compiler_core::BaseError<CompileErrorType>;

/// Compile a given source code into a bytecode object.
pub fn compile(
    source: &str,
    mode: compile::Mode,
    source_path: String,
    opts: CompileOpts,
) -> Result<CodeObject, CompileError> {
    let mut ast = match parser::parse(source, mode.into(), &source_path) {
        Ok(x) => x,
        Err(e) => return Err(e.into()),
    };
    if opts.optimize > 0 {
        ast = ConstantOptimizer::new()
            .fold_mod(ast)
            .unwrap_or_else(|e| match e {});
    }
    compile::compile_top(&ast, source_path, mode, opts).map_err(|e| e.into())
}

pub fn compile_symtable(
    source: &str,
    mode: compile::Mode,
    source_path: &str,
) -> Result<symboltable::SymbolTable, CompileError> {
    let res = match mode {
        compile::Mode::Exec | compile::Mode::Single | compile::Mode::BlockExpr => {
            let ast = parser::parse_program(source, source_path).map_err(|e| e.into())?;
            symboltable::SymbolTable::scan_program(&ast)
        }
        compile::Mode::Eval => {
            let expr = parser::parse_expression(source, source_path).map_err(|e| e.into())?;
            symboltable::SymbolTable::scan_expr(&expr)
        }
    };
    res.map_err(|e| e.into_codegen_error(source_path.to_owned()).into())
}
