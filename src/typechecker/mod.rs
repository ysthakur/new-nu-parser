use crate::compiler::Compiler;
use crate::errors::{Severity, SourceError};
use crate::parser::{AstNode, NodeId};
use std::cmp::Ordering;
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypeVarId(pub usize);

struct TypeVar {
    lower_bound: TypeId,
    upper_bound: TypeId,
}

/// To represent the top type, make everything inside empty
#[derive(Debug, Default, Clone, PartialEq, Eq, Hash)]
struct Conj {
    list: Option<Dnf>,
    record: Vec<(String, Dnf)>,
    type_vars: Vec<TypeVarId>,
    /// Something like Type::Int, Type::String, etc. included verbatim
    verbatim: Option<TypeId>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Hash)]
struct DnfPart {
    any: bool,
    pos: Conj,
    neg: Conj,
}

/// To represent the bottom type, use an empty vec
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Dnf(Vec<DnfPart>);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypeId(pub usize);

/// Input/output type pair of a closure/command
#[derive(Debug, Clone)]
pub struct InOutType {
    pub in_type: TypeId,
    pub out_type: TypeId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordTypeId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OneOfId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InterTypeId(pub usize);

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
    Record(RecordTypeId),
    OneOf(OneOfId),
    Error,

    Top,
    Bottom,
    TypeVarRef(TypeVarId),
    Neg(TypeId),
    And(TypeId, TypeId),
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

pub const TOP_TYPE: TypeId = TypeId(15);
pub const BOTTOM_TYPE: TypeId = TypeId(15);

pub struct Typechecker<'a> {
    /// Immutable reference to a compiler after the name binding pass
    compiler: &'a Compiler,

    /// Types referenced by TypeId
    types: Vec<Type>,

    /// Types of nodes. Each type in this vector matches a node in compiler.ast_nodes at the same position.
    pub node_types: Vec<TypeId>,
    /// Record fields used for `RecordType`. Each value in this vector matches with the index in RecordTypeId.
    /// The individual field lists are stored sorted by field name.
    pub record_types: Vec<Vec<(NodeId, TypeId)>>,
    /// Types used for `OneOf`. Each value in this vector matches with the index in OneOfId
    pub oneof_types: Vec<HashSet<TypeId>>,
    /// Type of each Variable in compiler.variables, indexed by VarId
    pub variable_types: Vec<TypeId>,
    /// Input/output type pairs of each declaration in compiler.decls, indexed by DeclId
    pub decl_types: Vec<Vec<InOutType>>,
    /// Errors encountered during type checking
    pub errors: Vec<SourceError>,

    /// Indexed by TypeVarId
    type_vars: Vec<TypeVar>,
    /// Subtype relations. Each item is (subtype, supertype)
    subtypes: HashSet<(Dnf, Dnf)>,
    /// Assumed subtype relations (they refer to this as the "later" modality
    /// and use a right arrow to represent it)
    subtypes_assum: HashSet<(Dnf, Dnf)>,
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
                Type::Top,
                Type::Bottom,
            ],
            node_types: vec![UNKNOWN_TYPE; compiler.ast_nodes.len()],
            record_types: Vec::new(),
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
            type_vars: vec![],
            subtypes: HashSet::new(),
            subtypes_assum: HashSet::new(),
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
        if !self.compiler.ast_nodes.is_empty() {
            let last = self.compiler.ast_nodes.len() - 1;
            let last_node_id = NodeId(last);
            self.typecheck_node(last_node_id);

            println!("=== Subtype bounds ===");
            for (sub, supe) in self.subtypes.clone() {
                let sub = self.dnf_to_ty(&sub);
                let supe = self.dnf_to_ty(&supe);
                println!("{} {}", self.type_to_string(sub), self.type_to_string(supe));
            }
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

    fn is_expr(&self, node_id: NodeId) -> bool {
        match self.compiler.ast_nodes[node_id.0] {
            AstNode::Null
            | AstNode::Int
            | AstNode::Float
            | AstNode::True
            | AstNode::False
            | AstNode::String
            | AstNode::List(_)
            | AstNode::Record { .. }
            | AstNode::Block(_)
            | AstNode::Closure { .. }
            | AstNode::BinaryOp { .. }
            | AstNode::Variable
            | AstNode::If { .. }
            | AstNode::Call { .. }
            | AstNode::Match { .. } => true,
            _ => false,
        }
    }

    fn infer(&mut self, node_id: NodeId) -> TypeId {
        match self.compiler.ast_nodes[node_id.0] {
            AstNode::Null => {
                self.set_node_type_id(node_id, NOTHING_TYPE);
            }
            AstNode::Int => {
                self.set_node_type_id(node_id, INT_TYPE);
            }
            AstNode::Float => {
                self.set_node_type_id(node_id, FLOAT_TYPE);
            }
            AstNode::True | AstNode::False => {
                self.set_node_type_id(node_id, BOOL_TYPE);
            }
            AstNode::String => {
                self.set_node_type_id(node_id, STRING_TYPE);
            }
            AstNode::List(ref items) => {
                let list_ty = self.fresh_var();
                let res = self.push_type(Type::TypeVarRef(list_ty));
                for item in items {
                    let item_ty = self.infer(*item);
                    self.constrain(item_ty, res);
                }
                self.set_node_type_id(node_id, res);
            }
            AstNode::Record { ref pairs } => {
                let mut field_types = pairs
                    .iter()
                    .map(|(name, value)| (*name, self.infer(*value)))
                    .collect::<Vec<_>>();
                field_types.sort_by_cached_key(|(name, _)| self.compiler.get_span_contents(*name));

                self.record_types.push(field_types);
                let ty_id = self.push_type(Type::Record(RecordTypeId(self.record_types.len() - 1)));
                self.set_node_type_id(node_id, ty_id);
            }
            AstNode::Block(block_id) => {
                let block = &self.compiler.blocks[block_id.0];

                for inner_node_id in &block.nodes {
                    self.typecheck_node(*inner_node_id);
                }

                // Block type is the type of the last statement, since blocks
                // by themselves aren't supposed to be typed
                let block_type = block
                    .nodes
                    .last()
                    .map_or(NONE_TYPE, |node_id| self.type_id_of(*node_id));

                self.set_node_type_id(node_id, block_type);
            }
            AstNode::Closure { params, block } => {
                // TODO: input/output types
                if let Some(params_node_id) = params {
                    self.typecheck_node(params_node_id);
                }

                self.typecheck_node(block);
                self.set_node_type_id(node_id, CLOSURE_TYPE);
            }
            AstNode::BinaryOp { lhs, op, rhs } => self.typecheck_binary_op(lhs, op, rhs, node_id),
            AstNode::Variable => {
                let var_id = self
                    .compiler
                    .var_resolution
                    .get(&node_id)
                    .expect("missing resolved variable");

                self.set_node_type_id(node_id, self.variable_types[var_id.0]);
            }
            AstNode::If {
                condition,
                then_block,
                else_block,
            } => {
                let cond_ty = self.infer(condition);
                self.constrain(cond_ty, BOOL_TYPE);

                let then_ty = self.infer(then_block);

                if let Some(else_blk) = else_block {
                    let else_ty = self.infer(else_blk);
                    let if_ty = self.fresh_var();
                    let res = self.push_type(Type::TypeVarRef(if_ty));
                    self.constrain(then_ty, res);
                    self.constrain(else_ty, res);
                    self.set_node_type_id(node_id, res);
                } else {
                    self.set_node_type_id(node_id, then_ty);
                }
            }
            AstNode::Call { ref parts } => self.typecheck_call(parts, node_id),
            AstNode::Match {
                ref target,
                ref match_arms,
            } => {
                todo!()
            }
            _ => self.error(
                format!(
                    "Can't infer type for non-expression node: '{:?}'",
                    self.compiler.ast_nodes[node_id.0]
                ),
                node_id,
            ),
        };

        self.node_types[node_id.0]
    }

    /// Constrain `lhs` to be a subtype of `rhs`
    fn constrain(&mut self, lhs: TypeId, rhs: TypeId) {
        let lhs_dnf = self.dnf(lhs);
        let rhs_dnf = self.dnf(rhs);
        if self.subtypes.contains(&(lhs_dnf.clone(), rhs_dnf.clone())) {
            // Subtyping relation already exists, no need to do anything (C-Hyp)
            return;
        }
        // Otherwise, follow C-Assum
        self.subtypes_assum.insert((lhs_dnf, rhs_dnf));
        let neg_rhs = self.push_type(Type::Neg(rhs));
        let lhs_and_neg_rhs = self.push_type(Type::And(lhs, neg_rhs));
        let dnf = self.dnf(lhs_and_neg_rhs);
        self.constrain_bottom(dnf);
    }

    /// Constrain the given type to be a subtype of the bottom type
    fn constrain_bottom(&mut self, ty: Dnf) {
        for part in ty.0.iter() {
            self.constrain_bottom_part(part);
        }
    }

    fn constrain_bottom_part(&mut self, part: &DnfPart) {
        // C-NotBot
        if part.pos == Conj::default() {
            panic!("Doesn't typecheck");
        }

        // TODO constraints involving int, float, number

        for field in part.neg.record.iter() {
            if part.pos.record.contains(field) {
                // TODO implement C-Rcd1
            } else {
                panic!("C-Rcd2 failed");
            }
        }

        let mut part = part.clone();

        // C-Var1
        while !part.pos.type_vars.is_empty() {
            let var = part.pos.type_vars.pop().unwrap();

            let c_ty = self.dnf_to_ty(&Dnf(vec![part.clone()]));
            let not_c = self.push_type(Type::Neg(c_ty));

            self.subtypes.insert((
                Dnf(vec![DnfPart {
                    pos: Conj {
                        type_vars: vec![var],
                        ..Default::default()
                    },
                    ..Default::default()
                }]),
                self.dnf(not_c),
            ));

            let lb = self.lower_bound(var);
            self.constrain(lb, not_c);
        }

        // C-Var2
        while !part.neg.type_vars.is_empty() {
            let var = part.neg.type_vars.pop().unwrap();

            let c_ty = self.dnf_to_ty(&Dnf(vec![part.clone()]));

            self.subtypes.insert((
                self.dnf(c_ty),
                Dnf(vec![DnfPart {
                    pos: Conj {
                        type_vars: vec![var],
                        ..Default::default()
                    },
                    ..Default::default()
                }]),
            ));

            let ub = self.upper_bound(var);
            self.constrain(c_ty, ub);
        }
    }

    fn dnf_to_ty(&mut self, dnf: &Dnf) -> TypeId {
        let mut res = HashSet::new();

        for part in dnf.0.iter() {
            if part.pos == Conj::default() {
                res.insert(self.conj_to_ty(&part.neg));
            } else {
                let pos = self.conj_to_ty(&part.pos);
                if part.neg == Conj::default() {
                    res.insert(pos);
                } else {
                    let neg = self.conj_to_ty(&part.neg);
                    res.insert(self.push_type(Type::And(pos, neg)));
                }
            };
        }

        if res.is_empty() {
            BOTTOM_TYPE
        } else if res.len() == 1 {
            *res.iter().next().unwrap()
        } else {
            let oneof_id = OneOfId(self.oneof_types.len());
            self.oneof_types.push(res);
            self.push_type(Type::OneOf(oneof_id))
        }
    }

    fn conj_to_ty(&mut self, conj: &Conj) -> TypeId {
        let mut vars = conj.type_vars.iter();
        let vars_ty = if let Some(first) = vars.next() {
            Some(
                vars.fold(self.push_type(Type::TypeVarRef(*first)), |acc, var| {
                    let ty = self.push_type(Type::TypeVarRef(*var));
                    self.push_type(Type::And(acc, ty))
                }),
            )
        } else {
            None
        };

        let rest_ty = if let Some(inner) = &conj.list {
            let inner_ty = self.dnf_to_ty(inner);
            self.push_type(Type::List(inner_ty))
        } else if let Some(ty) = &conj.verbatim {
            *ty
        } else {
            TOP_TYPE
        };

        if let Some(vars_ty) = vars_ty {
            if rest_ty == TOP_TYPE {
                vars_ty
            } else {
                self.push_type(Type::And(vars_ty, rest_ty))
            }
        } else {
            rest_ty
        }
    }

    fn lower_bound(&mut self, var: TypeVarId) -> TypeId {
        let mut subs = HashSet::new();

        for (sub, supe) in self.subtypes.clone() {
            let Dnf(parts) = supe;
            if parts.len() == 1 {
                let DnfPart {
                    pos: Conj { type_vars, .. },
                    ..
                } = &parts[0];
                if type_vars.contains(&var) {
                    subs.insert(self.dnf_to_ty(&sub));
                }
            }
        }

        if subs.is_empty() {
            BOTTOM_TYPE
        } else if subs.len() == 1 {
            *subs.iter().next().unwrap()
        } else {
            let oneof_id = OneOfId(self.oneof_types.len());
            self.oneof_types.push(subs);
            self.push_type(Type::OneOf(oneof_id))
        }
    }

    fn upper_bound(&mut self, var: TypeVarId) -> TypeId {
        let mut res = TOP_TYPE;

        for (sub, supe) in self.subtypes.clone() {
            let Dnf(parts) = sub;
            if parts.len() == 1 {
                let DnfPart {
                    pos: Conj { type_vars, .. },
                    ..
                } = &parts[0];
                if type_vars.contains(&var) {
                    let supe = self.dnf_to_ty(&supe);
                    if res == TOP_TYPE {
                        res = supe;
                    } else {
                        res = self.push_type(Type::And(res, supe));
                    }
                }
            }
        }

        res
    }

    fn dnf(&self, ty_id: TypeId) -> Dnf {
        match self.types[ty_id.0] {
            Type::Top => Dnf(vec![Default::default()]),
            Type::Bottom => Dnf(vec![]),
            Type::TypeVarRef(var) => Dnf(vec![DnfPart {
                pos: Conj {
                    type_vars: vec![var],
                    ..Default::default()
                },
                ..Default::default()
            }]),
            Type::Record(record_ty_id) => {
                let fields = self.record_types[record_ty_id.0]
                    .iter()
                    .map(|(node, ty)| {
                        (
                            String::from_utf8_lossy(self.compiler.get_span_contents(*node))
                                .to_string(),
                            self.dnf(*ty),
                        )
                    })
                    .collect::<Vec<_>>();
                Dnf(vec![DnfPart {
                    pos: Conj {
                        record: fields,
                        ..Default::default()
                    },
                    ..Default::default()
                }])
            }
            Type::And(lhs, rhs) => self.inter_dnf(&self.dnf(lhs), &self.dnf(rhs)),
            Type::OneOf(oneof_id) => {
                let mut types = self.oneof_types[oneof_id.0].iter();
                let mut res = self.dnf(*types.next().expect("oneof must have at least one type"));
                for ty in types {
                    let mut parts = res.0;
                    parts.extend(self.dnf(*ty).0);
                    res = self.union(&parts);
                }
                res
            }
            Type::Int
            | Type::Float
            | Type::Number
            | Type::Bool
            | Type::String
            | Type::Binary
            | Type::None
            | Type::Nothing
            | Type::Closure
            | Type::Error => Dnf(vec![DnfPart {
                pos: Conj {
                    verbatim: Some(ty_id),
                    ..Default::default()
                },
                ..Default::default()
            }]),
            Type::List(elem_ty) => Dnf(vec![DnfPart {
                pos: Conj {
                    list: Some(self.dnf(elem_ty)),
                    ..Default::default()
                },
                ..Default::default()
            }]),
            Type::Stream(elem_ty) => todo!(),
            Type::Any | Type::Unknown => Dnf(vec![DnfPart {
                any: true,
                ..Default::default()
            }]),
            Type::Neg(ty_id) => self.dnf_neg(ty_id),
            Type::Forbidden => unreachable!(),
        }
    }

    /// Get the DNF of the *negation* of `ty_id`
    fn dnf_neg(&self, ty_id: TypeId) -> Dnf {
        match self.types[ty_id.0] {
            Type::Top => self.dnf(BOTTOM_TYPE),
            Type::Bottom => self.dnf(TOP_TYPE),
            Type::Neg(ty_id) => self.dnf(ty_id),
            Type::And(lhs, rhs) => {
                let lhs = self.dnf_neg(lhs);
                let rhs = self.dnf_neg(rhs);
                let mut parts = lhs.0;
                parts.extend(rhs.0);
                self.union(&parts)
            }
            Type::OneOf(oneof_id) => {
                let types = self.oneof_types[oneof_id.0].clone();
                let mut types = types.iter();
                let first = *types.next().expect("oneof must have at least one type");
                let mut res = self.dnf_neg(first);
                for ty in types {
                    res = self.inter_dnf(&res, &self.dnf_neg(*ty));
                }
                res
            }
            Type::TypeVarRef(var) => Dnf(vec![DnfPart {
                neg: Conj {
                    type_vars: vec![var],
                    ..Default::default()
                },
                ..Default::default()
            }]),
            Type::Record(record_ty_id) => {
                let fields = self.record_types[record_ty_id.0]
                    .iter()
                    .map(|(node, ty)| {
                        (
                            String::from_utf8_lossy(self.compiler.get_span_contents(*node))
                                .to_string(),
                            self.dnf(*ty),
                        )
                    })
                    .collect::<Vec<_>>();
                Dnf(vec![DnfPart {
                    neg: Conj {
                        record: fields,
                        ..Default::default()
                    },
                    ..Default::default()
                }])
            }
            Type::Int
            | Type::Float
            | Type::Number
            | Type::Bool
            | Type::String
            | Type::Binary
            | Type::None
            | Type::Nothing
            | Type::Closure
            | Type::Error => Dnf(vec![DnfPart {
                neg: Conj {
                    verbatim: Some(ty_id),
                    ..Default::default()
                },
                ..Default::default()
            }]),
            Type::List(elem_ty) => Dnf(vec![DnfPart {
                neg: Conj {
                    list: Some(self.dnf(elem_ty)),
                    ..Default::default()
                },
                ..Default::default()
            }]),
            Type::Unknown | Type::Any => Dnf(vec![Default::default()]),
            Type::Forbidden => todo!(),
            Type::Stream(type_id) => todo!(),
        }
    }

    fn union(&self, parts: &[DnfPart]) -> Dnf {
        let mut res = vec![];
        for part in parts {
            // TODO actually implement this
            res.push(part.clone());
        }

        Dnf(res)
    }

    fn inter_dnf(&self, lhs: &Dnf, rhs: &Dnf) -> Dnf {
        let mut res = vec![];
        for l in lhs.0.iter() {
            for r in rhs.0.iter() {
                if let Some(part) = self.inter_dnf_part(l, r) {
                    res.push(part);
                } else {
                    return Dnf(vec![]);
                }
            }
        }

        return Dnf(res);
    }

    fn inter_dnf_part(&self, lhs: &DnfPart, rhs: &DnfPart) -> Option<DnfPart> {
        let mut lhs = lhs.clone();

        for var in rhs.pos.type_vars.iter() {
            if lhs.neg.type_vars.contains(var) {
                return None;
            }
            if !lhs.pos.type_vars.contains(var) {
                lhs.pos.type_vars.push(*var);
            }
        }
        for var in rhs.neg.type_vars.iter() {
            if lhs.pos.type_vars.contains(var) {
                return None;
            }
            if !lhs.neg.type_vars.contains(var) {
                lhs.neg.type_vars.push(*var);
            }
        }

        if let Some(rhs_ty) = rhs.pos.verbatim {
            match (lhs.pos.verbatim, rhs_ty) {
                (Some(INT_TYPE | FLOAT_TYPE | NUMBER_TYPE), NUMBER_TYPE) => {
                    lhs.pos.verbatim = Some(NUMBER_TYPE);
                }
                (Some(NUMBER_TYPE), INT_TYPE | FLOAT_TYPE) => {
                    lhs.pos.verbatim = Some(NUMBER_TYPE);
                }
                (Some(lhs_ty), _) => {
                    if lhs_ty != rhs_ty {
                        return None;
                    }
                }
                (_, _) => lhs.pos.verbatim = Some(rhs_ty),
            }
            match (lhs.neg.verbatim, rhs_ty) {
                (Some(INT_TYPE | FLOAT_TYPE | NUMBER_TYPE), NUMBER_TYPE)
                | (Some(NUMBER_TYPE), INT_TYPE | FLOAT_TYPE) => return None,
                (Some(lhs_ty), _) if lhs_ty == rhs_ty => return None,
                _ => {}
            }
        }
        if let Some(rhs_ty) = rhs.neg.verbatim {
            match (lhs.neg.verbatim, rhs_ty) {
                (Some(INT_TYPE | FLOAT_TYPE | NUMBER_TYPE), NUMBER_TYPE) => {
                    lhs.neg.verbatim = Some(NUMBER_TYPE);
                }
                (Some(NUMBER_TYPE), INT_TYPE | FLOAT_TYPE) => {
                    lhs.neg.verbatim = Some(NUMBER_TYPE);
                }
                (Some(lhs_ty), _) => {
                    if lhs_ty != rhs_ty {
                        return None;
                    }
                }
                (_, _) => lhs.neg.verbatim = Some(rhs_ty),
            }
            match (lhs.pos.verbatim, rhs_ty) {
                (Some(INT_TYPE | FLOAT_TYPE | NUMBER_TYPE), NUMBER_TYPE)
                | (Some(NUMBER_TYPE), INT_TYPE | FLOAT_TYPE) => return None,
                (Some(lhs_ty), _) if lhs_ty == rhs_ty => return None,
                _ => {}
            }
        }

        if let Some(rhs_lst) = &rhs.pos.list {
            if let Some(lhs_lst) = &lhs.pos.list {
                let inter = self.inter_dnf(&lhs_lst, &rhs_lst);
                if inter.0.is_empty() {
                    // The intersection is uninhabited (bottom type)
                    return None;
                }
                lhs.pos.list = Some(inter);
            } else {
                lhs.pos.list = Some(rhs_lst.clone());
            }
            if let Some(lhs_lst) = &lhs.neg.list {
                if !self.inter_dnf(&lhs_lst, &rhs_lst).0.is_empty() {
                    // Intersection should be empty (bottom type)
                    return None;
                }
            }
        }
        if let Some(rhs_lst) = &rhs.neg.list {
            if let Some(lhs_lst) = lhs.neg.list {
                let inter = self.inter_dnf(&lhs_lst, &rhs_lst);
                if inter.0.is_empty() {
                    // The intersection is uninhabited (bottom type)
                    return None;
                }
                lhs.neg.list = Some(inter);
            } else {
                lhs.neg.list = Some(rhs_lst.clone());
            }
            if let Some(lhs_lst) = lhs.pos.list {
                if !self.inter_dnf(&lhs_lst, &rhs_lst).0.is_empty() {
                    // Intersection should be empty (bottom type)
                    return None;
                }
            }
        }

        // todo merge records

        None
    }

    fn typecheck_node(&mut self, node_id: NodeId) {
        if self.is_expr(node_id) {
            self.infer(node_id);
            return;
        }
        match self.compiler.ast_nodes[node_id.0] {
            AstNode::Params(ref params) => {
                for param in params {
                    self.typecheck_node(*param);
                }
                // Params are not supposed to be evaluated
                self.set_node_type_id(node_id, FORBIDDEN_TYPE);
            }
            AstNode::Param { name, ty } => {
                if let Some(ty) = ty {
                    self.typecheck_node(ty);

                    let var_id = self
                        .compiler
                        .var_resolution
                        .get(&name)
                        .expect("missing resolved variable");
                    self.variable_types[var_id.0] = self.type_id_of(ty);
                    self.set_node_type_id(node_id, self.type_id_of(ty));
                } else {
                    self.set_node_type_id(node_id, ANY_TYPE);
                }
            }
            AstNode::Type {
                name,
                args,
                optional,
            } => {
                let ty_id = self.typecheck_type(name, args, optional);
                self.set_node_type_id(node_id, ty_id);
            }
            AstNode::RecordType {
                fields,
                optional: _, // TODO handle optional record types
            } => {
                let AstNode::Params(field_nodes) = self.compiler.get_node(fields) else {
                    panic!("internal error: record fields aren't Params");
                };
                let mut fields = field_nodes
                    .iter()
                    .map(|field| {
                        let AstNode::Param { name, ty } = self.compiler.get_node(*field) else {
                            panic!("internal error: record field isn't Param");
                        };
                        let ty_id = match ty {
                            Some(ty) => {
                                self.typecheck_node(*ty);
                                self.type_id_of(*ty)
                            }
                            None => ANY_TYPE,
                        };
                        (*name, ty_id)
                    })
                    .collect::<Vec<_>>();
                // Store fields sorted by name
                fields.sort_by_cached_key(|(name, _)| self.compiler.get_span_contents(*name));

                self.record_types.push(fields);
                let ty_id = self.push_type(Type::Record(RecordTypeId(self.record_types.len() - 1)));
                self.set_node_type_id(node_id, ty_id);
            }
            AstNode::TypeArgs(ref args) => {
                for arg in args {
                    self.typecheck_node(*arg);
                }
                // Type argument lists are not supposed to be evaluated
                self.set_node_type_id(node_id, FORBIDDEN_TYPE);
            }
            AstNode::Block(block_id) => {
                let block = &self.compiler.blocks[block_id.0];

                for inner_node_id in &block.nodes {
                    self.typecheck_node(*inner_node_id);
                }

                // Block type is the type of the last statement, since blocks
                // by themselves aren't supposed to be typed
                let block_type = block
                    .nodes
                    .last()
                    .map_or(NONE_TYPE, |node_id| self.type_id_of(*node_id));

                self.set_node_type_id(node_id, block_type);
            }
            AstNode::Let {
                variable_name,
                ty,
                initializer,
                is_mutable: _,
            } => self.typecheck_let(variable_name, ty, initializer, node_id),
            AstNode::Def {
                name,
                params,
                in_out_types,
                block,
            } => self.typecheck_def(name, params, in_out_types, block, node_id),
            AstNode::Alias { new_name, old_name } => {
                self.typecheck_alias(new_name, old_name, node_id)
            }
            AstNode::For {
                variable,
                range,
                block,
            } => {
                // We don't need to typecheck variable after this
                self.typecheck_node(range);

                let var_id = self
                    .compiler
                    .var_resolution
                    .get(&variable)
                    .expect("missing resolved variable");
                if let Type::List(type_id) = self.type_of(range) {
                    self.variable_types[var_id.0] = type_id;
                    self.set_node_type_id(variable, type_id);
                } else {
                    self.variable_types[var_id.0] = ANY_TYPE;
                    self.set_node_type_id(variable, ERROR_TYPE);
                    self.error("For loop range is not a list", range);
                }

                self.typecheck_node(block);
                if self.type_id_of(block) != NONE_TYPE {
                    self.error("Blocks in looping constructs cannot return values", block);
                }

                if self.type_id_of(node_id) != ERROR_TYPE {
                    self.set_node_type_id(node_id, NONE_TYPE);
                }
            }
            AstNode::While { condition, block } => {
                self.typecheck_node(block);
                if self.type_id_of(block) != NONE_TYPE {
                    self.error("Blocks in looping constructs cannot return values", block);
                }

                self.typecheck_node(condition);

                // the condition should always evaluate to a boolean
                if self.type_of(condition) != Type::Bool {
                    self.error("The condition for while loop is not a boolean", condition);
                    self.set_node_type_id(node_id, ERROR_TYPE);
                } else {
                    self.set_node_type_id(node_id, self.type_id_of(block));
                }
            }
            _ => self.error(
                format!(
                    "unsupported ast node '{:?}' in typechecker",
                    self.compiler.ast_nodes[node_id.0]
                ),
                node_id,
            ),
        }
    }

    fn typecheck_match(
        &mut self,
        target: &NodeId,
        match_arms: &Vec<(NodeId, NodeId)>,
    ) -> HashSet<TypeId> {
        self.typecheck_node(*target);

        let mut output_types = HashSet::new();
        // typecheck each node
        let target_id = self.type_id_of(*target);
        for (match_node, result_node) in match_arms {
            self.typecheck_node(*match_node);
            self.typecheck_node(*result_node);

            let match_id = self.type_id_of(*match_node);
            match (self.type_of(*target), self.type_of(*match_node)) {
                // First is of type Any which will always match
                (Type::Any, _) => {
                    self.add_resolved_types(&mut output_types, &self.type_id_of(*result_node));
                }
                // Same as above but for second
                (_, Type::Any) => {
                    self.add_resolved_types(&mut output_types, &self.type_id_of(*result_node));
                }
                // the second is one of the possible types of the first
                (Type::OneOf(id), _) if self.oneof_types[id.0].contains(&match_id) => {
                    self.add_resolved_types(&mut output_types, &self.type_id_of(*result_node));
                }
                // the first is one of the possible types of the second
                (_, Type::OneOf(id)) if self.oneof_types[id.0].contains(&target_id) => {
                    self.add_resolved_types(&mut output_types, &self.type_id_of(*result_node));
                }
                // the both the target and the one matched against are
                // oneof<many types> then we need to check if they have any type in common
                (Type::OneOf(id1), Type::OneOf(id2)) => {
                    if self.oneof_types[id1.0]
                        .intersection(&self.oneof_types[id2.0])
                        .count()
                        != 0
                    {
                        self.add_resolved_types(&mut output_types, &self.type_id_of(*result_node));
                    } else {
                        self.error("The target to be matched against and the possible types of the matched arm are completely disjoint", *match_node);
                    }
                }
                // Check if the two types can be matched
                (target_id, match_id) if self.is_type_compatible(target_id, match_id) => {
                    self.add_resolved_types(&mut output_types, &self.type_id_of(*result_node));
                }
                _ => {
                    self.error("The types do not match", *match_node);
                }
            }
        }
        output_types
    }

    fn typecheck_binary_op(&mut self, lhs: NodeId, op: NodeId, rhs: NodeId, node_id: NodeId) {
        self.typecheck_node(lhs);
        self.typecheck_node(rhs);
        self.set_node_type_id(op, FORBIDDEN_TYPE);

        let lhs_id = self.type_id_of(lhs);
        let rhs_id = self.type_id_of(rhs);
        let lhs_type = self.type_of(lhs);
        let rhs_type = self.type_of(rhs);

        let out_type = match self.compiler.ast_nodes[op.0] {
            AstNode::Equal | AstNode::NotEqual => Some(Type::Bool),
            AstNode::LessThan
            | AstNode::GreaterThan
            | AstNode::LessThanOrEqual
            | AstNode::GreaterThanOrEqual => {
                self.constrain(lhs_id, NUMBER_TYPE);
                self.constrain(rhs_id, NUMBER_TYPE);
                if check_numeric_op(lhs_type, rhs_type) == Type::Unknown {
                    self.binary_op_err("comparison", lhs, op, rhs);
                    None
                } else {
                    Some(Type::Bool)
                }
            }
            AstNode::Minus
            | AstNode::Multiply
            | AstNode::Divide
            | AstNode::FloorDiv
            | AstNode::Modulo
            | AstNode::Pow => {
                self.constrain(lhs_id, NUMBER_TYPE);
                self.constrain(rhs_id, NUMBER_TYPE);
                let type_id = check_numeric_op(lhs_type, rhs_type);

                if type_id == Type::Unknown {
                    self.binary_op_err("math operation", lhs, op, rhs);
                    None
                } else {
                    Some(type_id)
                }
            }
            AstNode::RegexMatch | AstNode::NotRegexMatch => {
                self.constrain(lhs_id, STRING_TYPE);
                self.constrain(rhs_id, STRING_TYPE);
                match (lhs_type, rhs_type) {
                    (Type::String | Type::Any, Type::String | Type::Any) => Some(Type::Bool),
                    _ => {
                        self.binary_op_err("string operation", lhs, op, rhs);
                        None
                    }
                }
            }
            AstNode::In => match rhs_type {
                Type::String => match lhs_type {
                    Type::String | Type::Any => Some(Type::Bool),
                    _ => {
                        self.binary_op_err("string operation", lhs, op, rhs);
                        None
                    }
                },
                Type::List(elem_ty) => {
                    if self.is_type_compatible(lhs_type, self.types[elem_ty.0]) {
                        Some(Type::Bool)
                    } else {
                        self.binary_op_err("list operation", lhs, op, rhs);
                        None
                    }
                }
                Type::Any => Some(Type::Bool),
                _ => {
                    self.binary_op_err("list/string operation", lhs, op, rhs);
                    None
                }
            },
            AstNode::And | AstNode::Xor | AstNode::Or => {
                self.constrain(lhs_id, BOOL_TYPE);
                self.constrain(rhs_id, BOOL_TYPE);
                match (lhs_type, rhs_type) {
                    (Type::Bool, Type::Bool) => Some(Type::Bool),
                    _ => {
                        self.binary_op_err("logical operation", lhs, op, rhs);
                        None
                    }
                }
            }
            AstNode::Plus => {
                let ty = check_plus_op(lhs_type, rhs_type);

                if ty == Type::Unknown {
                    self.binary_op_err("addition", lhs, op, rhs);
                    None
                } else {
                    Some(ty)
                }
            }
            AstNode::Append => {
                let lhs_type = self.type_of(lhs);
                let rhs_type = self.type_of(rhs);

                match (lhs_type, rhs_type) {
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
                        self.binary_op_err("append", lhs, op, rhs);
                        None
                    }
                }
            }
            AstNode::Assignment
            | AstNode::AddAssignment
            | AstNode::SubtractAssignment
            | AstNode::MultiplyAssignment
            | AstNode::DivideAssignment
            | AstNode::AppendAssignment => Some(Type::None),
            _ => panic!("internal error: unsupported node passed as binary op: {op:?}"),
        };

        if let Some(ty) = out_type {
            self.set_node_type(node_id, ty);
        } else {
            self.set_node_type_id(node_id, ERROR_TYPE);
        }
    }

    fn typecheck_def(
        &mut self,
        name: NodeId,
        params: NodeId,
        in_out_types: Option<NodeId>,
        block: NodeId,
        node_id: NodeId,
    ) {
        let in_out_types = in_out_types
            .map(|ty| {
                let AstNode::InOutTypes(types) = self.compiler.get_node(ty) else {
                    panic!("internal error: return type is not a return type");
                };
                types
                    .iter()
                    .map(|ty| {
                        let AstNode::InOutType(in_ty, out_ty) = self.compiler.get_node(*ty) else {
                            panic!("internal error: return type is not a return type");
                        };
                        let AstNode::Type {
                            name: in_name,
                            args: in_args,
                            optional: in_optional,
                        } = *self.compiler.get_node(*in_ty)
                        else {
                            panic!("internal error: type is not a type");
                        };
                        let AstNode::Type {
                            name: out_name,
                            args: out_args,
                            optional: out_optional,
                        } = *self.compiler.get_node(*out_ty)
                        else {
                            panic!("internal error: type is not a type");
                        };
                        InOutType {
                            in_type: self.typecheck_type(in_name, in_args, in_optional),
                            out_type: self.typecheck_type(out_name, out_args, out_optional),
                        }
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        self.typecheck_node(params);
        self.typecheck_node(block);
        self.set_node_type_id(node_id, NONE_TYPE);

        // set input/output types for the command
        let decl_id = self
            .compiler
            .decl_resolution
            .get(&name)
            .expect("missing declared decl");

        if in_out_types.is_empty() {
            self.decl_types[decl_id.0] = vec![InOutType {
                in_type: ANY_TYPE,
                out_type: self.type_id_of(block),
            }];
        } else {
            // TODO check that block output type matches expected type
            self.decl_types[decl_id.0] = in_out_types;
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

    fn typecheck_call(&mut self, parts: &[NodeId], node_id: NodeId) {
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
            if matches!(self.compiler.ast_nodes[part.0], AstNode::Name) {
                self.set_node_type_id(*part, STRING_TYPE);
            } else {
                self.typecheck_node(*part);
            }
        }
    }

    fn typecheck_let(
        &mut self,
        variable_name: NodeId,
        ty: Option<NodeId>,
        initializer: NodeId,
        node_id: NodeId,
    ) {
        self.typecheck_node(initializer);

        if let Some(ty) = ty {
            self.typecheck_node(ty);

            if !self.is_type_compatible(self.type_of(ty), self.type_of(initializer)) {
                self.error("initializer does not match declared type", initializer)
            }
        }

        let var_id = self
            .compiler
            .var_resolution
            .get(&variable_name)
            .expect("missing declared variable");

        let type_id = if let Some(ty) = ty {
            self.type_id_of(ty)
        } else {
            self.type_id_of(initializer)
        };

        self.variable_types[var_id.0] = type_id;
        self.set_node_type_id(variable_name, type_id);
        self.set_node_type_id(node_id, NONE_TYPE);
    }

    fn typecheck_type(
        &mut self,
        name_id: NodeId,
        args_id: Option<NodeId>,
        _optional: bool,
    ) -> TypeId {
        let name = self.compiler.get_span_contents(name_id);

        // taken from parse_shape_name() in Nushell:
        match name {
            b"any" => ANY_TYPE,
            // b"binary" => SyntaxShape::Binary,
            // b"block" => // not possible to pass blocks
            b"list" => {
                if let Some(args_id) = args_id {
                    self.typecheck_node(args_id);

                    if let AstNode::TypeArgs(args) = self.compiler.get_node(args_id) {
                        if args.len() > 1 {
                            let types =
                                String::from_utf8_lossy(self.compiler.get_span_contents(args_id));
                            self.error(format!("list must have only one type argument (to allow selection of types, use oneof{} -- WIP)", types), args_id);
                            self.push_type(Type::List(UNKNOWN_TYPE))
                        } else if args.is_empty() {
                            self.error("list must have one type argument", args_id);
                            self.push_type(Type::List(UNKNOWN_TYPE))
                        } else {
                            let args_ty_id = self.type_id_of(args[0]);
                            self.push_type(Type::List(args_ty_id))
                        }
                    } else {
                        panic!("args are not args");
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

    fn set_node_type(&mut self, node_id: NodeId, ty: Type) {
        let type_id = self.push_type(ty);
        self.node_types[node_id.0] = type_id;
    }

    fn set_node_type_id(&mut self, node_id: NodeId, type_id: TypeId) {
        self.node_types[node_id.0] = type_id;
    }

    fn fresh_var(&mut self) -> TypeVarId {
        self.type_vars.push(TypeVar {
            lower_bound: BOTTOM_TYPE,
            upper_bound: TOP_TYPE,
        });
        TypeVarId(self.type_vars.len() - 1)
    }

    /// Finds a "supertype" of two types (e.g., number for float and int)
    fn least_common_type(&mut self, lhs: Type, rhs: Type) -> Type {
        match (lhs, rhs) {
            (Type::List(lhs_id), Type::List(rhs_id)) => {
                let item_type = self.least_common_type(self.types[lhs_id.0], self.types[rhs_id.0]);
                let item_type_id = self.push_type(item_type);
                Type::List(item_type_id)
            }
            (Type::Record(lhs_id), Type::Record(rhs_id)) => {
                let mut common_fields = Vec::new();

                let mut l = 0;
                let mut r = 0;
                while l < self.record_types[lhs_id.0].len() && r < self.record_types[rhs_id.0].len()
                {
                    let (lhs_name, lhs_ty) = self.record_types[lhs_id.0][l];
                    let (rhs_name, rhs_ty) = self.record_types[rhs_id.0][r];
                    let lhs_text = self.compiler.get_span_contents(lhs_name);
                    let rhs_text = self.compiler.get_span_contents(rhs_name);
                    match lhs_text.cmp(rhs_text) {
                        Ordering::Less => {
                            l += 1;
                        }
                        Ordering::Greater => {
                            r += 1;
                        }
                        Ordering::Equal => {
                            let field_ty =
                                self.least_common_type(self.types[lhs_ty.0], self.types[rhs_ty.0]);
                            let field_ty_id = self.push_type(field_ty);
                            common_fields.push((lhs_name, field_ty_id));
                            l += 1;
                            r += 1;
                        }
                    }
                }

                self.record_types.push(common_fields);
                Type::Record(RecordTypeId(self.record_types.len() - 1))
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
            Type::Record(id) => {
                let mut fmt = "record<".to_string();
                let types = &self.record_types[id.0];
                for (name, ty) in types {
                    fmt += &String::from_utf8_lossy(self.compiler.get_span_contents(*name));
                    fmt += ": ";
                    fmt += &self.type_to_string(*ty);
                    fmt += ", ";
                }
                if !types.is_empty() {
                    fmt.pop();
                    fmt.pop();
                }
                fmt.push('>');
                fmt
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
            Type::TypeVarRef(_) => "var".to_string(),
            Type::Error => "error".to_string(),
            Type::Top => "top".to_string(),
            Type::Bottom => "bottom".to_string(),
            Type::Neg(ty_id) => format!("~{}", self.type_to_string(*ty_id)),
            Type::And(lhs, rhs) => format!(
                "{} & {}",
                self.type_to_string(*lhs),
                self.type_to_string(*rhs)
            ),
        }
    }

    /// Check if one type can be cast to another type
    fn is_type_compatible(&self, lhs: Type, rhs: Type) -> bool {
        match (lhs, rhs) {
            (Type::Int, Type::Number) => true,
            (Type::Float, Type::Number) => true,
            (Type::Number, Type::Int) => true,
            (Type::Number, Type::Float) => true,
            (Type::Any, _) => true,
            (_, Type::Any) => true,
            (Type::Record(lhs_id), Type::Record(rhs_id)) => {
                let lhs_fields = &self.record_types[lhs_id.0];
                let rhs_fields = &self.record_types[rhs_id.0];

                let mut l = 0;
                let mut r = 0;
                while l < lhs_fields.len() && r < rhs_fields.len() {
                    let (lhs_name, lhs_ty) = lhs_fields[l];
                    let (rhs_name, rhs_ty) = rhs_fields[r];
                    let lhs_text = self.compiler.get_span_contents(lhs_name);
                    let rhs_text = self.compiler.get_span_contents(rhs_name);
                    match lhs_text.cmp(rhs_text) {
                        Ordering::Less => {
                            l += 1;
                        }
                        Ordering::Greater => {
                            r += 1;
                        }
                        Ordering::Equal => {
                            if !self.is_type_compatible(self.types[lhs_ty.0], self.types[rhs_ty.0])
                            {
                                return false;
                            }
                            l += 1;
                            r += 1;
                        }
                    }
                }
                true
            }
            _ => lhs == rhs,
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
