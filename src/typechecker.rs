use crate::compiler::Compiler;
use crate::errors::{Severity, SourceError};
use crate::parser::{
    BinOp, BlockId, Def, Expr, ExprHandle, Handle, NodeId, Param, Stmt, StmtHandle, TypeHandle,
};
use std::cmp::Ordering;
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypeId(pub usize);

/// Input/output type pair of a closure/command
#[derive(Debug, Clone)]
pub struct InOutType {
    pub in_type: TypeId,
    pub out_type: TypeId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OneOfId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Type {
    /// Any node that hasn't been touched by the typechecker will have this type
    Unknown,
    /// Some nodes shouldn't be directly evaluated (like operators). These will have a "forbidden"
    /// to differentiate them from the "unknown" type.
    Forbidden,
    /// None type means that a node has no type. For example, statements like let x = ... do not
    /// output anything and thus don't have any type.
    None,
    Any,
    Number,
    Nothing,
    Int,
    Float,
    Bool,
    String,
    Binary,
    Closure,
    List(TypeId),
    Stream(TypeId),
    OneOf(OneOfId),
    Error,
}

pub struct Types {
    pub types: Vec<Type>,
    pub node_types: Vec<TypeId>,
    pub errors: Vec<SourceError>,
}

// The below are predefined simple types hardcoded into the Typechecker to avoid re-adding them all
// the time:

pub const UNKNOWN_TYPE: TypeId = TypeId(0);
pub const FORBIDDEN_TYPE: TypeId = TypeId(1);
pub const NONE_TYPE: TypeId = TypeId(2);
pub const ANY_TYPE: TypeId = TypeId(3);
pub const NUMBER_TYPE: TypeId = TypeId(4);
pub const NOTHING_TYPE: TypeId = TypeId(5);
pub const INT_TYPE: TypeId = TypeId(6);
pub const FLOAT_TYPE: TypeId = TypeId(7);
pub const BOOL_TYPE: TypeId = TypeId(8);
pub const STRING_TYPE: TypeId = TypeId(9);
pub const BINARY_TYPE: TypeId = TypeId(10);
pub const CLOSURE_TYPE: TypeId = TypeId(11);

// Common composite types can be hardcoded as well, like list<any>:

pub const LIST_ANY_TYPE: TypeId = TypeId(12);
pub const BYTE_STREAM_TYPE: TypeId = TypeId(13);
pub const ERROR_TYPE: TypeId = TypeId(14);

pub struct Typechecker<'a> {
    /// Immutable reference to a compiler after the name binding pass
    compiler: &'a Compiler<'a>,

    /// Types referenced by TypeId
    types: Vec<Type>,

    /// Types of nodes. Each type in this vector matches a node in compiler.nodes at the same position.
    pub node_types: Vec<TypeId>,
    /// Types used for `OneOf`. Each value in this vector matches with the index in OneOfId
    pub oneof_types: Vec<HashSet<TypeId>>,
    /// Type of each Variable in compiler.variables, indexed by VarId
    pub variable_types: Vec<TypeId>,
    /// Input/output type pairs of each declaration in compiler.decls, indexed by DeclId
    pub decl_types: Vec<Vec<InOutType>>,
    /// Errors encountered during type checking
    pub errors: Vec<SourceError>,
}

impl<'a> Typechecker<'a> {
    pub fn new(compiler: &'a Compiler) -> Self {
        Self {
            compiler,
            types: vec![
                // The order must be the same as with the xxx_TYPE constants above
                Type::Unknown,
                Type::Forbidden,
                Type::None,
                Type::Any,
                Type::Number,
                Type::Nothing,
                Type::Int,
                Type::Float,
                Type::Bool,
                Type::String,
                Type::Binary,
                Type::Closure,
                Type::List(ANY_TYPE),
                Type::Stream(BINARY_TYPE),
                Type::Error,
            ],
            node_types: vec![UNKNOWN_TYPE; compiler.nodes.len()],
            oneof_types: Vec::new(),
            variable_types: vec![UNKNOWN_TYPE; compiler.variables.len()],
            decl_types: vec![
                vec![InOutType {
                    in_type: ANY_TYPE,
                    out_type: ANY_TYPE,
                }];
                compiler.decls.len()
            ],
            errors: vec![],
        }
    }

    pub fn to_types(self) -> Types {
        Types {
            types: self.types,
            node_types: self.node_types,
            errors: self.errors,
        }
    }

    pub fn print(&self) {
        let output = self.display_state();
        print!("{output}");
    }

    pub fn display_state(&self) -> String {
        let mut result = String::new();

        result.push_str("==== TYPES ====\n");

        for (idx, node_type_id) in self.node_types.iter().enumerate() {
            result.push_str(&format!(
                "{}: {}\n",
                idx,
                self.type_to_string(*node_type_id)
            ));
        }

        if !self.errors.is_empty() {
            result.push_str("==== TYPE ERRORS ====\n");
            for error in &self.errors {
                result.push_str(&format!(
                    "{:?} (NodeId {}): {}\n",
                    error.severity, error.node_id.0, error.message
                ));
            }
        }

        result
    }

    /// Typecheck AST nodes, starting from the last node
    pub fn typecheck(&mut self) {
        for entry in &self.compiler.entry_points {
            self.typecheck_block(*entry.node);
        }
    }

    /// Get type ID of a node
    pub fn type_id_of(&self, node_id: NodeId) -> TypeId {
        self.node_types[node_id.0]
    }

    /// Get type of node
    pub fn type_of(&self, node_id: NodeId) -> Type {
        let type_id = self.type_id_of(node_id);
        self.types[type_id.0]
    }

    fn typecheck_block(&mut self, block_id: BlockId) -> TypeId {
        let stmts = &self.compiler.blocks[block_id.0].nodes;
        for stmt in stmts {
            self.typecheck_stmt(stmt.clone());
        }

        // Block type is the type of the last statement, since blocks
        // by themselves aren't supposed to be typed
        stmts
            .last()
            .map_or(NONE_TYPE, |handle| self.type_id_of(handle.id))
    }

    fn typecheck_stmt(&mut self, stmt: StmtHandle<'a>) {
        let node_id = stmt.id;
        self.set_node_type_id(node_id, NONE_TYPE);
        match stmt.node {
            Stmt::Def(def) => self.typecheck_def(def.clone(), node_id),
            Stmt::Alias { new_name, old_name } => {
                self.typecheck_alias(*new_name, *old_name, node_id)
            }
            Stmt::Let {
                variable_name,
                ty,
                initializer,
                is_mutable: _,
            } => self.typecheck_let(*variable_name, ty.clone(), initializer.clone(), node_id),
            Stmt::For {
                variable,
                range,
                block,
            } => {
                // We don't need to typecheck variable after this
                let range_ty_id = self.typecheck_expr(range.clone());
                let range_ty = self.types[range_ty_id.0];

                let var_id = self
                    .compiler
                    .var_resolution
                    .get(&variable)
                    .expect("missing resolved variable");
                if let Type::List(type_id) = range_ty {
                    self.variable_types[var_id.0] = type_id;
                    self.set_node_type_id(*variable, type_id);
                } else {
                    self.variable_types[var_id.0] = ANY_TYPE;
                    self.set_node_type_id(*variable, ERROR_TYPE);
                    self.error("For loop range is not a list", range.id);
                }

                if self.typecheck_block(*block.node) != NONE_TYPE {
                    self.error(
                        "Blocks in looping constructs cannot return values",
                        block.id,
                    );
                }

                if self.type_id_of(node_id) != ERROR_TYPE {
                    self.set_node_type_id(node_id, NONE_TYPE);
                }
            }
            Stmt::While { condition, block } => {
                let block_ty = self.typecheck_block(*block.node);
                if block_ty != NONE_TYPE {
                    self.error(
                        "Blocks in looping constructs cannot return values",
                        block.id,
                    );
                }

                // the condition should always evaluate to a boolean
                if self.typecheck_expr(condition.clone()) != BOOL_TYPE {
                    self.error(
                        "The condition for while loop is not a boolean",
                        condition.id,
                    );
                    self.set_node_type_id(node_id, ERROR_TYPE);
                } else {
                    self.set_node_type_id(node_id, block_ty);
                }
            }
            Stmt::Loop { block } => {
                let block_ty = self.typecheck_block(*block.node);
                if block_ty != NONE_TYPE {
                    self.error(
                        "Blocks in looping constructs cannot return values",
                        block.id,
                    );
                }
            }
            Stmt::Return(expr) => {
                if let Some(expr) = expr {
                    self.typecheck_expr(expr.clone());
                }
            }
            Stmt::Expr(expr) => {
                self.typecheck_expr(expr.clone());
            }
            Stmt::Break | Stmt::Continue | Stmt::Garbage => {}
        }
    }

    fn typecheck_expr(&mut self, expr: ExprHandle<'a>) -> TypeId {
        let node_id = expr.id;
        match expr.node {
            Expr::Null => self.set_node_type_id(node_id, NOTHING_TYPE),
            Expr::Int => self.set_node_type_id(node_id, INT_TYPE),
            Expr::Float => self.set_node_type_id(node_id, FLOAT_TYPE),
            Expr::True | Expr::False => self.set_node_type_id(node_id, BOOL_TYPE),
            Expr::String { .. } => self.set_node_type_id(node_id, STRING_TYPE),
            Expr::List(ref items) => {
                if let Some(first) = items.first() {
                    self.typecheck_expr(first.clone());
                    let first_type = self.type_of(first.id);

                    let mut all_numbers = is_type_compatible(first_type, Type::Number);
                    let mut all_same = true;

                    for item in items.iter().skip(1) {
                        self.typecheck_expr(item.clone());
                        let item_type = self.type_of(item.id);

                        if all_numbers && !is_type_compatible(item_type, Type::Number) {
                            all_numbers = false;
                        }

                        if all_same && item_type != first_type {
                            all_same = false;
                        }
                    }

                    if all_same {
                        self.set_node_type(node_id, Type::List(self.type_id_of(first.id)))
                    } else if all_numbers {
                        self.set_node_type(node_id, Type::List(NUMBER_TYPE))
                    } else {
                        self.set_node_type_id(node_id, LIST_ANY_TYPE)
                    }
                } else {
                    self.set_node_type_id(node_id, LIST_ANY_TYPE)
                }
            }
            Expr::Record { pairs } => {
                // TODO
                UNKNOWN_TYPE
            }
            Expr::Table { header, rows } => {
                // TODO
                UNKNOWN_TYPE
            }
            Expr::Range { lhs, rhs } => {
                // TODO
                UNKNOWN_TYPE
            }
            Expr::Block(block_id) => {
                let block_ty = self.typecheck_block(*block_id);
                self.set_node_type_id(node_id, block_ty)
            }
            Expr::Closure { params, block } => {
                // TODO: input/output types
                if let Some(params) = params {
                    for param in &params.node.0 {
                        self.typecheck_param(param.clone());
                    }
                }

                self.typecheck_block(*block.node);
                self.set_node_type_id(node_id, CLOSURE_TYPE)
            }
            Expr::BinaryOp { lhs, op, rhs } => {
                self.typecheck_binary_op(lhs.clone(), op.clone(), rhs.clone(), node_id)
            }
            Expr::VarRef => {
                let var_id = self
                    .compiler
                    .var_resolution
                    .get(&node_id)
                    .expect("missing resolved variable");

                self.set_node_type_id(node_id, self.variable_types[var_id.0])
            }
            Expr::If {
                condition,
                then_block,
                else_block,
            } => {
                let cond_ty = self.typecheck_expr(condition.clone());

                let then_type_id = self.typecheck_block(*then_block.node);
                let mut else_type = None;

                if let Some(else_blk) = else_block {
                    let ty_id = self.typecheck_expr(else_blk.clone());
                    else_type = Some(self.types[ty_id.0]);
                }

                let mut types = HashSet::new();
                self.add_resolved_types(&mut types, &then_type_id);

                if let Some(Type::OneOf(id)) = else_type {
                    types.extend(self.oneof_types[id.0].iter());
                } else if else_type.is_none() {
                    types.insert(NONE_TYPE);
                } else {
                    types.insert(self.type_id_of(else_block.as_ref().expect("Already checked").id));
                }

                // the condition should always evaluate to a boolean
                if cond_ty != BOOL_TYPE {
                    self.error("The condition for if branch is not a boolean", condition.id);
                    self.set_node_type_id(node_id, ERROR_TYPE)
                } else if types.len() > 1 {
                    self.oneof_types.push(types);
                    self.set_node_type(node_id, Type::OneOf(OneOfId(self.oneof_types.len() - 1)))
                } else {
                    self.set_node_type_id(node_id, *types.iter().next().expect("Can't be empty"))
                }
            }
            Expr::Call { ref parts } => self.typecheck_call(parts, node_id),
            Expr::Match {
                ref target,
                ref match_arms,
            } => {
                // Check all the output types of match
                let output_types = self.typecheck_match(target.clone(), match_arms);
                match output_types.len().cmp(&1) {
                    Ordering::Greater => {
                        self.oneof_types.push(output_types);
                        self.set_node_type(
                            node_id,
                            Type::OneOf(OneOfId(self.oneof_types.len() - 1)),
                        )
                    }
                    Ordering::Equal => self.set_node_type_id(
                        node_id,
                        *output_types
                            .iter()
                            .next()
                            .expect("Will contain one element"),
                    ),
                    Ordering::Less => self.set_node_type_id(node_id, NOTHING_TYPE),
                }
            }
            Expr::NamedValue { name, value } => todo!(),
            Expr::MemberAccess { target, field } => todo!(),
            Expr::Garbage => ERROR_TYPE,
        }
    }

    fn typecheck_match(
        &mut self,
        target: ExprHandle<'a>,
        match_arms: &Vec<(ExprHandle<'a>, ExprHandle<'a>)>,
    ) -> HashSet<TypeId> {
        self.typecheck_expr(target.clone());

        let mut output_types = HashSet::new();
        // typecheck each node
        let target_id = self.type_id_of(target.id);
        for (match_node, result_node) in match_arms {
            self.typecheck_expr(match_node.clone());
            self.typecheck_expr(result_node.clone());

            let match_id = self.type_id_of(match_node.id);
            match (self.type_of(target.id), self.type_of(match_node.id)) {
                // First is of type Any which will always match
                (Type::Any, _) => {
                    self.add_resolved_types(&mut output_types, &self.type_id_of(result_node.id));
                }
                // Same as above but for second
                (_, Type::Any) => {
                    self.add_resolved_types(&mut output_types, &self.type_id_of(result_node.id));
                }
                // the second is one of the possible types of the first
                (Type::OneOf(id), _) if self.oneof_types[id.0].contains(&match_id) => {
                    self.add_resolved_types(&mut output_types, &self.type_id_of(result_node.id));
                }
                // the first is one of the possible types of the second
                (_, Type::OneOf(id)) if self.oneof_types[id.0].contains(&target_id) => {
                    self.add_resolved_types(&mut output_types, &self.type_id_of(result_node.id));
                }
                // the both the target and the one matched against are
                // oneof<many types> then we need to check if they have any type in common
                (Type::OneOf(id1), Type::OneOf(id2)) => {
                    if self.oneof_types[id1.0]
                        .intersection(&self.oneof_types[id2.0])
                        .count()
                        != 0
                    {
                        self.add_resolved_types(
                            &mut output_types,
                            &self.type_id_of(result_node.id),
                        );
                    } else {
                        self.error("The target to be matched against and the possible types of the matched arm are completely disjoint", match_node.id);
                    }
                }
                // Check if the two types can be matched
                (target_id, match_id) if is_type_compatible(target_id, match_id) => {
                    self.add_resolved_types(&mut output_types, &self.type_id_of(result_node.id));
                }
                _ => {
                    self.error("The types do not match", match_node.id);
                }
            }
        }
        output_types
    }

    fn typecheck_binary_op(
        &mut self,
        lhs: ExprHandle<'a>,
        op: Handle<'a, BinOp>,
        rhs: ExprHandle<'a>,
        node_id: NodeId,
    ) -> TypeId {
        let lhs_type = self.typecheck_expr(lhs.clone());
        let lhs_type = self.types[lhs_type.0];
        let rhs_type = self.typecheck_expr(rhs.clone());
        let rhs_type = self.types[rhs_type.0];

        let out_type = match op.node {
            BinOp::Equal | BinOp::NotEqual => Some(Type::Bool),
            BinOp::LessThan
            | BinOp::GreaterThan
            | BinOp::LessThanOrEqual
            | BinOp::GreaterThanOrEqual => {
                if check_numeric_op(lhs_type, rhs_type) == Type::Unknown {
                    self.binary_op_err("comparison", lhs.id, op.id, rhs.id);
                    None
                } else {
                    Some(Type::Bool)
                }
            }
            BinOp::Minus
            | BinOp::Multiply
            | BinOp::Divide
            | BinOp::FloorDiv
            | BinOp::Modulo
            | BinOp::Pow => {
                let type_id = check_numeric_op(lhs_type, rhs_type);

                if type_id == Type::Unknown {
                    self.binary_op_err("math operation", lhs.id, op.id, rhs.id);
                    None
                } else {
                    Some(type_id)
                }
            }
            BinOp::RegexMatch | BinOp::NotRegexMatch => match (lhs_type, rhs_type) {
                (Type::String | Type::Any, Type::String | Type::Any) => Some(Type::Bool),
                _ => {
                    self.binary_op_err("string operation", lhs.id, op.id, rhs.id);
                    None
                }
            },
            BinOp::In => match rhs_type {
                Type::String => match lhs_type {
                    Type::String | Type::Any => Some(Type::Bool),
                    _ => {
                        self.binary_op_err("string operation", lhs.id, op.id, rhs.id);
                        None
                    }
                },
                Type::List(elem_ty) => {
                    if is_type_compatible(lhs_type, self.types[elem_ty.0]) {
                        Some(Type::Bool)
                    } else {
                        self.binary_op_err("list operation", lhs.id, op.id, rhs.id);
                        None
                    }
                }
                Type::Any => Some(Type::Bool),
                _ => {
                    self.binary_op_err("list/string operation", lhs.id, op.id, rhs.id);
                    None
                }
            },
            BinOp::And | BinOp::Xor | BinOp::Or => match (lhs_type, rhs_type) {
                (Type::Bool, Type::Bool) => Some(Type::Bool),
                _ => {
                    self.binary_op_err("logical operation", lhs.id, op.id, rhs.id);
                    None
                }
            },
            BinOp::Plus => {
                let ty = check_plus_op(lhs_type, rhs_type);

                if ty == Type::Unknown {
                    self.binary_op_err("addition", lhs.id, op.id, rhs.id);
                    None
                } else {
                    Some(ty)
                }
            }
            BinOp::Append => match (lhs_type, rhs_type) {
                (Type::List(lhs_item_id), Type::List(rhs_item_id)) => {
                    let lhs_item_type = self.types[lhs_item_id.0];
                    let rhs_item_type = self.types[rhs_item_id.0];
                    let common_type = self.least_common_type(lhs_item_type, rhs_item_type);
                    let common_type_id = self.push_type(common_type);
                    Some(Type::List(common_type_id))
                }
                (Type::List(item_id), rhs_type) => {
                    let item_type = self.types[item_id.0];
                    let common_type = self.least_common_type(item_type, rhs_type);
                    let common_type_id = self.push_type(common_type);
                    Some(Type::List(common_type_id))
                }
                (lhs_type, Type::List(item_id)) => {
                    let item_type = self.types[item_id.0];
                    let common_type = self.least_common_type(lhs_type, item_type);
                    let common_type_id = self.push_type(common_type);
                    Some(Type::List(common_type_id))
                }
                _ => {
                    self.binary_op_err("append", lhs.id, op.id, rhs.id);
                    None
                }
            },
            BinOp::Assignment
            | BinOp::AddAssignment
            | BinOp::SubtractAssignment
            | BinOp::MultiplyAssignment
            | BinOp::DivideAssignment
            | BinOp::AppendAssignment => Some(Type::None),
            _ => panic!("internal error: unsupported node passed as binary op: {op:?}"),
        };

        if let Some(ty) = out_type {
            self.set_node_type(node_id, ty)
        } else {
            self.set_node_type_id(node_id, ERROR_TYPE)
        }
    }

    fn typecheck_param(&mut self, param: Handle<'a, Param>) -> TypeId {
        if let Some(ty) = &param.node.ty {
            let var_id = self
                .compiler
                .var_resolution
                .get(&param.node.name)
                .expect("missing resolved variable");
            let ty = self.typecheck_type(ty.clone());
            self.variable_types[var_id.0] = ty;
            self.set_node_type_id(param.id, ty)
        } else {
            self.set_node_type_id(param.id, ANY_TYPE)
        }
    }

    fn typecheck_def(&mut self, def: Def<'a>, node_id: NodeId) {
        let return_ty = def
            .return_ty
            .map(|ty| {
                ty.node
                    .0
                    .iter()
                    .map(|ty| {
                        let crate::parser::InOutType(in_ty, out_ty) = ty.node;
                        InOutType {
                            in_type: self.typecheck_type(in_ty.clone()),
                            out_type: self.typecheck_type(out_ty.clone()),
                        }
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        for param in &def.params.node.0 {
            self.typecheck_param(param.clone());
        }
        let block_ty = self.typecheck_block(*def.block.node);
        self.set_node_type_id(node_id, NONE_TYPE);

        // set input/output types for the command
        let decl_id = self
            .compiler
            .decl_resolution
            .get(&def.name)
            .expect("missing declared decl");

        if return_ty.is_empty() {
            self.decl_types[decl_id.0] = vec![InOutType {
                in_type: ANY_TYPE,
                out_type: block_ty,
            }];
        } else {
            // TODO check that block output type matches expected type
            self.decl_types[decl_id.0] = return_ty;
        }
    }

    fn typecheck_alias(&mut self, new_name: NodeId, old_name: NodeId, node_id: NodeId) {
        self.set_node_type_id(node_id, NONE_TYPE);

        // set input/output types for the command
        let decl_id_new = self
            .compiler
            .decl_resolution
            .get(&new_name)
            .expect("missing declared new name for alias");

        let decl_id_old = self.compiler.decl_resolution.get(&old_name);

        self.decl_types[decl_id_new.0] = decl_id_old.map_or(
            vec![InOutType {
                in_type: ANY_TYPE,
                out_type: BYTE_STREAM_TYPE,
            }],
            |decl_id| self.decl_types[decl_id.0].clone(),
        );
    }

    fn typecheck_call(&mut self, parts: &[ExprHandle<'a>], node_id: NodeId) -> TypeId {
        let num_name_parts = if let Some(decl_id) = self.compiler.decl_resolution.get(&node_id) {
            // TODO: The type should be `oneof<all_possible_output_types>`
            self.set_node_type_id(node_id, ANY_TYPE);

            self.compiler.decls[decl_id.0].name().split(' ').count()
        } else {
            // external call
            self.node_types[node_id.0] = BYTE_STREAM_TYPE;
            1
        };

        for part in &parts[num_name_parts..] {
            if matches!(part.node, Expr::String { bareword: true }) {
                self.set_node_type_id(part.id, STRING_TYPE);
            } else {
                self.typecheck_expr(part.clone());
            }
        }

        self.type_id_of(node_id)
    }

    fn typecheck_let(
        &mut self,
        variable_name: NodeId,
        ty: Option<TypeHandle<'a>>,
        initializer: ExprHandle<'a>,
        node_id: NodeId,
    ) {
        let init_id = initializer.id;
        let init_ty = self.typecheck_expr(initializer);

        if let Some(ty) = ty.clone() {
            let resolved = self.typecheck_type(ty);

            if !is_type_compatible(self.types[resolved.0], self.types[init_ty.0]) {
                self.error("initializer does not match declared type", init_id)
            }
        }

        let var_id = self
            .compiler
            .var_resolution
            .get(&variable_name)
            .expect("missing declared variable");

        let type_id = if let Some(ty) = ty {
            self.type_id_of(ty.id)
        } else {
            self.type_id_of(init_id)
        };

        self.variable_types[var_id.0] = type_id;
        self.set_node_type_id(variable_name, type_id);
        self.set_node_type_id(node_id, NONE_TYPE);
    }

    fn typecheck_type(&mut self, ty: TypeHandle<'a>) -> TypeId {
        let name = self.compiler.get_span_contents(ty.node.name);

        // taken from parse_shape_name() in Nushell:
        match name {
            b"any" => ANY_TYPE,
            // b"binary" => SyntaxShape::Binary,
            // b"block" => // not possible to pass blocks
            b"list" => {
                if let Some(args_handle) = &ty.node.params {
                    let arg_ids = args_handle
                        .node
                        .0
                        .iter()
                        .map(|arg| self.typecheck_type(arg.clone()))
                        .collect::<Vec<_>>();

                    if arg_ids.len() > 1 {
                        let types = String::from_utf8_lossy(
                            self.compiler.get_span_contents(args_handle.id),
                        );
                        self.error(format!("list must have only one type parameter (to allow selection of types, use oneof{} -- WIP)", types), args_handle.id);
                        self.push_type(Type::List(UNKNOWN_TYPE))
                    } else if arg_ids.is_empty() {
                        self.error("list must have one type parameter", args_handle.id);
                        self.push_type(Type::List(UNKNOWN_TYPE))
                    } else {
                        self.push_type(Type::List(arg_ids[0]))
                    }
                } else {
                    LIST_ANY_TYPE
                }
            }
            b"bool" => BOOL_TYPE,
            // b"cell-path" => SyntaxShape::CellPath,
            b"closure" => CLOSURE_TYPE, //FIXME: Closures should have known output types
            // b"datetime" => SyntaxShape::DateTime,
            // b"directory" => SyntaxShape::Directory,
            // b"duration" => SyntaxShape::Duration,
            // b"error" => SyntaxShape::Error,
            b"float" => FLOAT_TYPE,
            // b"filesize" => SyntaxShape::Filesize,
            // b"glob" => SyntaxShape::GlobPattern,
            b"int" => INT_TYPE,
            // _ if bytes.starts_with(b"list") => parse_list_shape(working_set, bytes, span, use_loc),
            b"nothing" => NOTHING_TYPE,
            b"number" => NUMBER_TYPE,
            // b"path" => SyntaxShape::Filepath,
            // b"range" => SyntaxShape::Range,
            // _ if bytes.starts_with(b"record") => {
            //     parse_collection_shape(working_set, bytes, span, use_loc)
            // }
            b"string" => STRING_TYPE,
            // _ if bytes.starts_with(b"table") => {
            //     parse_collection_shape(working_set, bytes, span, use_loc)
            // }
            _ => {
                // if bytes.contains(&b'@') {
                //     // type with completion
                // } else {
                UNKNOWN_TYPE
                // }
            }
        }
    }

    /// Add a new type and return its ID. To save space, common types are not pushed and their ID is
    /// returned directly.
    fn push_type(&mut self, ty: Type) -> TypeId {
        match ty {
            Type::Unknown => UNKNOWN_TYPE,
            Type::Forbidden => FORBIDDEN_TYPE,
            Type::None => NONE_TYPE,
            Type::Any => ANY_TYPE,
            Type::Number => NUMBER_TYPE,
            Type::Nothing => NOTHING_TYPE,
            Type::Int => INT_TYPE,
            Type::Float => FLOAT_TYPE,
            Type::Bool => BOOL_TYPE,
            Type::String => STRING_TYPE,
            Type::Closure => CLOSURE_TYPE,
            Type::List(ANY_TYPE) => LIST_ANY_TYPE,
            _ => {
                self.types.push(ty);
                TypeId(self.types.len() - 1)
            }
        }
    }

    fn set_node_type(&mut self, node_id: NodeId, ty: Type) -> TypeId {
        let type_id = self.push_type(ty);
        self.node_types[node_id.0] = type_id;
        type_id
    }

    fn set_node_type_id(&mut self, node_id: NodeId, type_id: TypeId) -> TypeId {
        self.node_types[node_id.0] = type_id;
        type_id
    }

    /// Finds a "supertype" of two types (e.g., number for float and int)
    fn least_common_type(&mut self, lhs: Type, rhs: Type) -> Type {
        match (lhs, rhs) {
            (Type::List(lhs_id), Type::List(rhs_id)) => {
                let item_type = self.least_common_type(self.types[lhs_id.0], self.types[rhs_id.0]);
                let item_type_id = self.push_type(item_type);
                Type::List(item_type_id)
            }
            (Type::Int, Type::Float) => Type::Number,
            (Type::Int, Type::Number) => Type::Number,
            (Type::Float, Type::Float) => Type::Number,
            (Type::Float, Type::Number) => Type::Number,
            _ => {
                if lhs == rhs {
                    lhs
                } else {
                    Type::Any
                }
            }
        }
    }

    fn type_to_string(&self, type_id: TypeId) -> String {
        let ty = &self.types[type_id.0];

        match ty {
            Type::Unknown => "unknown".to_string(),
            Type::Forbidden => "forbidden".to_string(),
            Type::None => "()".to_string(),
            Type::Any => "any".to_string(),
            Type::Number => "number".to_string(),
            Type::Nothing => "nothing".to_string(),
            Type::Int => "int".to_string(),
            Type::Float => "float".to_string(),
            Type::Bool => "bool".to_string(),
            Type::Binary => "binary".to_string(),
            Type::String => "string".to_string(),
            Type::Closure => "closure".to_string(),
            Type::List(subtype_id) => {
                format!("list<{}>", self.type_to_string(*subtype_id))
            }
            Type::Stream(subtype_id) => {
                format!("stream<{}>", self.type_to_string(*subtype_id))
            }
            Type::OneOf(id) => {
                let mut fmt = "oneof<".to_string();
                let mut types: Vec<_> = self.oneof_types[id.0]
                    .iter()
                    .map(|ty| self.type_to_string(*ty) + ", ")
                    .collect();
                types.sort();
                for ty in &types {
                    fmt += ty;
                }
                if !types.is_empty() {
                    fmt.pop();
                    fmt.pop();
                }
                fmt.push('>');
                fmt
            }
            Type::Error => "error".to_string(),
        }
    }

    fn error(&mut self, msg: impl Into<String>, node_id: NodeId) {
        self.errors.push(SourceError {
            message: msg.into(),
            node_id,
            severity: Severity::Error,
        })
    }

    fn binary_op_err(&mut self, op_msg: &str, lhs: NodeId, op: NodeId, rhs: NodeId) {
        self.error(
            format!(
                "type mismatch: unsupported {} between {} and {}",
                op_msg,
                self.type_to_string(self.type_id_of(lhs)),
                self.type_to_string(self.type_id_of(rhs)),
            ),
            op,
        );
        self.set_node_type_id(op, ERROR_TYPE);
    }

    fn add_resolved_types(&mut self, types: &mut HashSet<TypeId>, ty: &TypeId) {
        if let Type::OneOf(id) = self.types[ty.0] {
            types.extend(self.oneof_types[id.0].clone());
        } else {
            types.insert(*ty);
        }
    }
}

/// Check if one type can be cast to another type
fn is_type_compatible(lhs: Type, rhs: Type) -> bool {
    match (lhs, rhs) {
        (Type::Int, Type::Number) => true,
        (Type::Float, Type::Number) => true,
        (Type::Number, Type::Int) => true,
        (Type::Number, Type::Float) => true,
        (Type::Any, _) => true,
        (_, Type::Any) => true,
        _ => lhs == rhs,
    }
}

/// Check whether two types can perform common numeric operations
fn check_numeric_op(lhs: Type, rhs: Type) -> Type {
    match (rhs, lhs) {
        (Type::Int, Type::Int) => Type::Int,
        (Type::Int, Type::Float) => Type::Float,
        (Type::Int, Type::Number) => Type::Number,
        (Type::Float, Type::Int) => Type::Float,
        (Type::Float, Type::Float) => Type::Float,
        (Type::Float, Type::Number) => Type::Float,
        (Type::Number, Type::Int) => Type::Number,
        (Type::Number, Type::Float) => Type::Float,
        (Type::Number, Type::Number) => Type::Number,
        (Type::Any, _) => Type::Number,
        (_, Type::Any) => Type::Number,
        // TODO: Differentiate error based on whether LHS supports the op or not (see type_check.rs)
        _ => Type::Unknown,
    }
}

/// Check whether two types can perform addition
fn check_plus_op(lhs: Type, rhs: Type) -> Type {
    match (rhs, lhs) {
        (Type::String, Type::String) => Type::String,
        _ => check_numeric_op(lhs, rhs),
    }
}
