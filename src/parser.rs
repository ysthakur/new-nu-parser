use crate::compiler::{Compiler, RollbackPoint, Span};
use crate::errors::{Severity, SourceError};
use crate::lexer::{Token, Tokens};

use tracy_client::span;

pub struct Parser<'a> {
    pub compiler: Compiler<'a>,
    tokens: Tokens,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlockId(pub usize);

impl Node for BlockId {}

pub type BlockHandle<'a> = Handle<'a, BlockId>;

#[derive(Debug, Clone)]
pub struct Block<'a> {
    pub nodes: Vec<StmtHandle<'a>>,
}

impl<'a> Block<'a> {
    pub fn new(nodes: Vec<StmtHandle<'a>>) -> Block<'a> {
        Block { nodes }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum BlockContext {
    /// This block is a whole block of code not wrapped in curlies (e.g., a file)
    Bare,
    /// This block is wrapped in curlies
    Curlies,
    /// This block should be parsed as part of a closure starting after closure params
    Closure,
}

#[derive(Debug)]
pub enum ParamsContext {
    /// Params for a command signature
    Squares,
    /// Params for a closure
    Pipes,
}

#[derive(Debug)]
pub enum BarewordContext {
    /// Bareword is a string (e.g., in a list)
    String,
    /// Bareword is a name (e.g., in a call position)
    Call,
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub struct Handle<'a, T> {
    pub id: NodeId,
    pub node: &'a T,
}

pub trait Node: std::fmt::Debug {
    fn bareword_like(&self) -> bool {
        false
    }
}

pub type ExprHandle<'a> = Handle<'a, Expr<'a>>;

#[derive(Debug, PartialEq, Clone)]
pub enum Expr<'a> {
    Int,
    Float,
    String {
        bareword: bool,
    },
    VarRef,

    // Booleans
    True,
    False,

    // Empty values
    Null,

    Closure {
        params: Option<Handle<'a, Params<'a>>>,
        block: BlockHandle<'a>,
    },

    Call {
        parts: Vec<ExprHandle<'a>>,
    },
    NamedValue {
        name: NodeId,
        value: ExprHandle<'a>,
    },
    BinaryOp {
        lhs: ExprHandle<'a>,
        op: Handle<'a, BinOp>,
        rhs: ExprHandle<'a>,
    },
    Range {
        lhs: ExprHandle<'a>,
        rhs: ExprHandle<'a>,
    },
    List(Vec<ExprHandle<'a>>),
    Table {
        header: ExprHandle<'a>,
        rows: Vec<ExprHandle<'a>>,
    },
    Record {
        pairs: Vec<(ExprHandle<'a>, ExprHandle<'a>)>,
    },
    MemberAccess {
        target: ExprHandle<'a>,
        field: NodeId,
    },
    Block(BlockId),
    If {
        condition: ExprHandle<'a>,
        then_block: BlockHandle<'a>,
        else_block: Option<ExprHandle<'a>>,
    },
    Match {
        target: ExprHandle<'a>,
        match_arms: Vec<(ExprHandle<'a>, ExprHandle<'a>)>,
    },

    Garbage,
}

impl<'a> Node for Expr<'a> {
    fn bareword_like(&self) -> bool {
        match self {
            Expr::Int | Expr::Float | Expr::VarRef | Expr::String { .. } => true,
            _ => false,
        }
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct Def<'a> {
    pub name: NodeId,
    pub params: Handle<'a, Params<'a>>,
    pub return_ty: Option<Handle<'a, InOutTypes<'a>>>,
    pub block: BlockHandle<'a>,
}

pub type StmtHandle<'a> = Handle<'a, Stmt<'a>>;

#[derive(Debug, PartialEq, Clone)]
pub enum Stmt<'a> {
    Let {
        variable_name: NodeId,
        ty: Option<TypeHandle<'a>>,
        initializer: ExprHandle<'a>,
        is_mutable: bool,
    },
    While {
        condition: ExprHandle<'a>,
        block: BlockHandle<'a>,
    },
    For {
        variable: NodeId,
        range: ExprHandle<'a>,
        block: BlockHandle<'a>,
    },
    Loop {
        block: BlockHandle<'a>,
    },
    Return(Option<ExprHandle<'a>>),
    Break,
    Continue,
    Expr(ExprHandle<'a>),
    Def(Def<'a>),
    Alias {
        new_name: NodeId,
        old_name: NodeId,
    },

    Garbage,
}
impl<'a> Node for Stmt<'a> {}

#[derive(Debug, PartialEq, Clone)]
pub struct Params<'a>(pub Vec<Handle<'a, Param<'a>>>);
impl<'a> Node for Params<'a> {}

#[derive(Debug, PartialEq, Clone)]
pub struct Param<'a> {
    pub name: NodeId,
    pub ty: Option<TypeHandle<'a>>,
}
impl<'a> Node for Param<'a> {}

#[derive(Debug, PartialEq, Clone)]
pub struct InOutTypes<'a>(pub Vec<Handle<'a, InOutType<'a>>>);
impl<'a> Node for InOutTypes<'a> {}

/// Input/output type pair for a command
#[derive(Debug, PartialEq, Clone)]
pub struct InOutType<'a>(pub TypeHandle<'a>, pub TypeHandle<'a>);
impl<'a> Node for InOutType<'a> {}

pub type TypeHandle<'a> = Handle<'a, Type<'a>>;
#[derive(Debug, PartialEq, Clone)]
pub struct Type<'a> {
    pub name: NodeId,
    pub params: Option<Handle<'a, TypeArgs<'a>>>,
    pub optional: bool,
}
impl<'a> Node for Type<'a> {}

#[derive(Debug, PartialEq, Clone)]
pub struct TypeArgs<'a>(pub Vec<TypeHandle<'a>>);
impl<'a> Node for TypeArgs<'a> {}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum BinOp {
    // Normal binary operators
    Pow,
    Multiply,
    Divide,
    FloorDiv,
    Modulo,
    Plus,
    Minus,
    Equal,
    NotEqual,
    LessThan,
    GreaterThan,
    LessThanOrEqual,
    GreaterThanOrEqual,
    RegexMatch,
    NotRegexMatch,
    In,
    Append,
    And,
    Xor,
    Or,

    // Assignments
    // TODO maybe move these into a separate enum
    Assignment,
    AddAssignment,
    SubtractAssignment,
    MultiplyAssignment,
    DivideAssignment,
    AppendAssignment,

    Unknown,
}
impl Node for BinOp {}

// TODO: All nodes with Vec<...> should be moved to their own ID (like BlockId) to allow Copy trait
#[derive(Debug, PartialEq, Clone)]
pub enum AstNode {
    Name,
    VarDecl,

    /// Long flag ('--' + one or more letters)
    FlagLong,
    /// Short flag ('-' + single letter)
    FlagShort,
    /// Group of short flags ('-' + more than 1 letters)
    FlagShortGroup,

    Garbage,
}

impl Node for AstNode {
    fn bareword_like(&self) -> bool {
        match self {
            AstNode::Name | AstNode::VarDecl => true,
            _ => false,
        }
    }
}

pub const ASSIGNMENT_PRECEDENCE: usize = 10;

impl BinOp {
    pub fn precedence(&self) -> usize {
        match self {
            BinOp::Pow => 100,
            BinOp::Multiply | BinOp::Divide | BinOp::FloorDiv | BinOp::Modulo => 95,
            BinOp::Plus | BinOp::Minus => 90,
            BinOp::LessThan
            | BinOp::LessThanOrEqual
            | BinOp::GreaterThan
            | BinOp::GreaterThanOrEqual
            | BinOp::Equal
            | BinOp::NotEqual
            | BinOp::RegexMatch
            | BinOp::NotRegexMatch
            | BinOp::In
            | BinOp::Append => 80,
            BinOp::And => 50,
            BinOp::Xor => 45,
            BinOp::Or => 40,
            BinOp::Assignment
            | BinOp::AddAssignment
            | BinOp::SubtractAssignment
            | BinOp::MultiplyAssignment
            | BinOp::DivideAssignment
            | BinOp::AppendAssignment => ASSIGNMENT_PRECEDENCE,
            BinOp::Unknown => 0,
        }
    }
}

impl<'a> Parser<'a> {
    pub fn new(compiler: Compiler<'a>, tokens: Tokens) -> Self {
        Self { compiler, tokens }
    }

    fn position(&mut self) -> usize {
        self.tokens.peek_span().start
    }

    fn get_span_end(&self, node_id: NodeId) -> usize {
        self.compiler.spans[node_id.0].end
    }

    pub fn parse(mut self) -> Compiler<'a> {
        let _span = span!();
        let entry = self.block(BlockContext::Bare);
        self.compiler.entry_points.push(entry);

        self.compiler
    }

    pub fn expression_or_assignment(&mut self) -> ExprHandle<'a> {
        let _span = span!();
        self.math_expression(true)
    }

    pub fn expression(&mut self) -> ExprHandle<'a> {
        let _span = span!();
        self.math_expression(false)
    }

    pub fn math_expression(&mut self, allow_assignment: bool) -> ExprHandle<'a> {
        let _span = span!();
        let mut expr_stack = Vec::<(Handle<'a, BinOp>, ExprHandle)>::new();

        let mut last_prec = 1000000;

        let span_start = self.position();

        // Check for special forms
        if self.is_keyword(b"if") {
            return self.if_expression();
        } else if self.is_keyword(b"match") {
            return self.match_expression();
        }
        // TODO
        // } else if self.is_keyword(b"where") {
        // }

        // Otherwise assume a math expression
        let mut leftmost = self.simple_expression(BarewordContext::Call);

        if self.is_equals() {
            if !allow_assignment {
                self.error("assignment found in expression");
            }
            let op = self.operator();

            let rhs = self.expression();
            let span_end = self.get_span_end(rhs.id);

            return self.create_node(
                Expr::BinaryOp {
                    lhs: leftmost,
                    op,
                    rhs,
                },
                span_start,
                span_end,
            );
        }

        while self.has_tokens() {
            if self.is_operator() {
                let missing_space_before_op = !self.is_horizontal_space();
                let op = self.operator();
                let missing_space_after_op = !self.is_horizontal_space();

                if missing_space_before_op {
                    self.error_on_node("missing space before operator", op.id);
                }

                if missing_space_after_op {
                    self.error_on_node("missing space after operator", op.id);
                }

                let op_prec = op.node.precedence();

                if op_prec == ASSIGNMENT_PRECEDENCE && !allow_assignment {
                    self.error_on_node("assignment found in expression", op.id);
                }

                let rhs = if self.is_simple_expression() {
                    self.simple_expression(BarewordContext::Call)
                } else {
                    self.my_error("incomplete math expression", Expr::Garbage)
                };

                while op_prec <= last_prec {
                    let Some((op, rhs)) = expr_stack.pop() else {
                        break;
                    };

                    last_prec = op.node.precedence();

                    if last_prec < op_prec {
                        expr_stack.push((op, rhs));
                        break;
                    }

                    // TODO merge these two branches together like they used to be
                    if let Some(l) = expr_stack.pop() {
                        let lhs = l.1;
                        let (span_start, span_end) = self.spanning(lhs.id, rhs.id);
                        expr_stack.push((
                            l.0,
                            self.create_node(Expr::BinaryOp { lhs, op, rhs }, span_start, span_end),
                        ));
                    } else {
                        let (span_start, span_end) = self.spanning(leftmost.id, rhs.id);
                        leftmost = self.create_node(
                            Expr::BinaryOp {
                                lhs: leftmost,
                                op,
                                rhs,
                            },
                            span_start,
                            span_end,
                        );
                    }
                }

                expr_stack.push((op, rhs));

                last_prec = op_prec;
            } else {
                break;
            }
        }

        while let Some((op, rhs)) = expr_stack.pop() {
            // TODO merge these two branches together like they used to be
            if let Some(l) = expr_stack.pop() {
                let lhs = l.1;
                let (span_start, span_end) = self.spanning(lhs.id, rhs.id);
                expr_stack.push((
                    l.0,
                    self.create_node(Expr::BinaryOp { lhs, op, rhs }, span_start, span_end),
                ));
            } else {
                let (span_start, span_end) = self.spanning(leftmost.id, rhs.id);
                leftmost = self.create_node(
                    Expr::BinaryOp {
                        lhs: leftmost,
                        op,
                        rhs,
                    },
                    span_start,
                    span_end,
                );
            }
        }

        leftmost
    }

    pub fn simple_expression(&mut self, bareword_context: BarewordContext) -> ExprHandle<'a> {
        let _span = span!();

        // skip comments and newlines
        while self.is_comment() || self.is_newline() {
            self.tokens.advance();
        }

        let span_start = self.position();

        let (token, span) = self.tokens.peek();

        let mut expr = match token {
            Token::LCurly => self.record_or_closure(),
            Token::LParen => {
                self.tokens.advance();
                if self.tokens.peek_token() == Token::RParen {
                    self.my_error("use null instead of ()", Expr::Garbage)
                } else {
                    let output = self.expression();
                    self.rparen();
                    output
                }
            }
            Token::LSquare => self.list_or_table(),
            Token::Int => self.advance_node(Expr::Int, span),
            Token::Float => self.advance_node(Expr::Float, span),
            Token::DoubleQuotedString => self.advance_node(Expr::String { bareword: false }, span),
            Token::SingleQuotedString => self.advance_node(Expr::String { bareword: false }, span),
            Token::Dollar => self.variable(),
            Token::Bareword => match self.compiler.get_span_contents_manual(span.start, span.end) {
                b"true" => self.advance_node(Expr::True, span),
                b"false" => self.advance_node(Expr::False, span),
                b"null" => self.advance_node(Expr::Null, span),
                _ => match bareword_context {
                    BarewordContext::String => self.bareword_string(),
                    BarewordContext::Call => self.call(),
                },
            },
            _ => self.my_error("incomplete expression", Expr::Garbage),
        };

        loop {
            if self.is_horizontal_space() {
                return expr;
            } else if self.is_dotdot() {
                // Range
                self.tokens.advance();

                if self.is_horizontal_space() {
                    // TODO: implement range from
                    //
                    // TODO: tweak the garbage location.
                    self.error("incomplete range");
                    return expr;
                } else {
                    let rhs = self.simple_expression(BarewordContext::String);
                    let span_end = self.get_span_end(rhs.id);

                    expr = self.create_node(Expr::Range { lhs: expr, rhs }, span_start, span_end);
                }
            } else if self.is_dot() {
                // Member access
                self.tokens.advance();

                if self.is_horizontal_space() {
                    self.error("missing path name");
                    return expr;
                }

                let name = self.name();

                let field_or_call = if self.is_lparen() {
                    self.variable().id
                } else {
                    name
                };
                let span_end = self.get_span_end(field_or_call);

                expr = self.create_node(
                    Expr::MemberAccess {
                        target: expr,
                        field: field_or_call,
                    },
                    span_start,
                    span_end,
                );
            } else {
                return expr;
            }
        }
    }

    pub fn advance_node<T: Node + 'a>(&mut self, node: T, span: Span) -> Handle<'a, T> {
        self.tokens.advance();
        self.create_node(node, span.start, span.end)
    }

    pub fn variable(&mut self) -> ExprHandle<'a> {
        if self.is_dollar() {
            let span_start = self.position();
            self.tokens.advance();

            if let (Token::Bareword, name_span) = self.tokens.peek() {
                self.tokens.advance();
                self.create_node(Expr::VarRef, span_start, name_span.end)
            } else {
                self.my_error("variable name must be a bareword", Expr::Garbage)
            }
        } else {
            self.my_error("expected variable starting with '$'", Expr::Garbage)
        }
    }

    pub fn variable_decl(&mut self) -> NodeId {
        let _span = span!();

        let span_start = self.position();

        if self.is_dollar() {
            self.tokens.advance();
        }

        if let (Token::Bareword, name_span) = self.tokens.peek() {
            self.tokens.advance();
            self.create_node(AstNode::VarDecl, span_start, name_span.end)
                .id
        } else {
            self.error("variable assignment name must be a bareword")
        }
    }

    pub fn call(&mut self) -> ExprHandle<'a> {
        let _span = span!();
        let mut parts = vec![self.call_name()];
        let mut is_head = true;
        let span_start = self.position();

        while self.has_tokens() {
            if self.is_newline() {
                break;
            }

            if self.is_name() && is_head {
                parts.push(self.bareword_string());
                continue;
            }

            // TODO: Add flags

            is_head = false;
            let arg_id = self.simple_expression(BarewordContext::String);
            parts.push(arg_id);
        }

        let span_end = self.position();

        self.create_node(Expr::Call { parts }, span_start, span_end)
    }

    pub fn list_or_table(&mut self) -> ExprHandle<'a> {
        let _span = span!();
        let span_start = self.position();
        let mut is_table = false;
        let mut items: Vec<ExprHandle> = vec![];

        self.lsquare();
        let mut span_end = self.position();

        loop {
            if self.is_rsquare() {
                span_end = self.position();
                self.tokens.advance();
                break;
            } else if self.is_comma() || self.is_newline() {
                // TODO: should we disallow `[,,,]`?
                self.tokens.advance();
            } else if self.is_semicolon() {
                if items.len() != 1 {
                    self.error("semicolon to create table should immediately follow headers");
                } else if !matches!(items[0].node, Expr::List(_)) {
                    self.error_on_node("tables require a list for their headers", items[0].id)
                }
                self.tokens.advance();
                is_table = true;
            } else if self.is_simple_expression() {
                items.push(self.simple_expression(BarewordContext::String));
            } else {
                items.push(self.my_error("expected list item", Expr::Garbage));
                if self.is_eof() {
                    // prevent forever looping if there is no token to put the error on
                    break;
                }
            }
        }

        if is_table {
            let header = items.remove(0);
            self.create_node(
                Expr::Table {
                    header,
                    rows: items,
                },
                span_start,
                span_end,
            )
        } else {
            self.create_node(Expr::List(items), span_start, span_end)
        }
    }

    pub fn record_or_closure(&mut self) -> ExprHandle<'a> {
        let _span = span!();
        let span_start = self.position();
        let mut span_end = self.position(); // TODO: make sure we only initialize it expectedly

        let mut is_closure = false;
        let mut first_pass = true;
        // For the record
        let mut items = vec![];

        self.lcurly();
        self.skip_newlines();

        // Explicit closure case
        if self.is_pipe() {
            let args = Some(self.signature_params(ParamsContext::Pipes));
            let block = self.block(BlockContext::Closure);
            self.rcurly();
            span_end = self.position();

            return self.create_node(
                Expr::Closure {
                    params: args,
                    block,
                },
                span_start,
                span_end,
            );
        }

        let rollback_point = self.get_rollback_point();
        loop {
            self.skip_newlines();
            if self.is_rcurly() {
                self.rcurly();
                span_end = self.position();
                break;
            }
            let key = self.simple_expression(BarewordContext::String);
            self.skip_newlines();
            if first_pass && !self.is_colon() {
                is_closure = true;
                break;
            }
            self.colon();
            self.skip_newlines();
            let val = self.simple_expression(BarewordContext::String);
            items.push((key, val));
            first_pass = false;

            if self.is_comma() {
                self.comma()
            }
            if self.is_eof() {
                // abort when appropriate
                break;
            }
        }

        if is_closure {
            self.apply_rollback(rollback_point);
            let block = self.block(BlockContext::Closure);
            self.rcurly();

            span_end = self.position();

            self.create_node(
                Expr::Closure {
                    params: None,
                    block,
                },
                span_start,
                span_end,
            )
        } else {
            self.create_node(Expr::Record { pairs: items }, span_start, span_end)
        }
    }

    pub fn operator(&mut self) -> Handle<'a, BinOp> {
        let (token, span) = self.tokens.peek();

        match token {
            Token::Plus => self.advance_node(BinOp::Plus, span),
            Token::PlusPlus => self.advance_node(BinOp::Append, span),
            Token::Dash => self.advance_node(BinOp::Minus, span),
            Token::Asterisk => self.advance_node(BinOp::Multiply, span),
            Token::ForwardSlash => self.advance_node(BinOp::Divide, span),
            Token::ForwardSlashForwardSlash => self.advance_node(BinOp::FloorDiv, span),
            Token::LessThan => self.advance_node(BinOp::LessThan, span),
            Token::LessThanEqual => self.advance_node(BinOp::LessThanOrEqual, span),
            Token::GreaterThan => self.advance_node(BinOp::GreaterThan, span),
            Token::GreaterThanEqual => self.advance_node(BinOp::GreaterThanOrEqual, span),
            Token::EqualsEquals => self.advance_node(BinOp::Equal, span),
            Token::ExclamationEquals => self.advance_node(BinOp::NotEqual, span),
            Token::EqualsTilde => self.advance_node(BinOp::RegexMatch, span),
            Token::ExclamationTilde => self.advance_node(BinOp::NotRegexMatch, span),
            Token::AsteriskAsterisk => self.advance_node(BinOp::Pow, span),
            Token::Equals => self.advance_node(BinOp::Assignment, span),
            Token::PlusEquals => self.advance_node(BinOp::AddAssignment, span),
            Token::DashEquals => self.advance_node(BinOp::SubtractAssignment, span),
            Token::AsteriskEquals => self.advance_node(BinOp::MultiplyAssignment, span),
            Token::ForwardSlashEquals => self.advance_node(BinOp::DivideAssignment, span),
            Token::PlusPlusEquals => self.advance_node(BinOp::AppendAssignment, span),
            Token::Bareword => match self.compiler.get_span_contents_manual(span.start, span.end) {
                b"mod" => self.advance_node(BinOp::Modulo, span),
                b"in" => self.advance_node(BinOp::In, span),
                b"and" => self.advance_node(BinOp::And, span),
                b"xor" => self.advance_node(BinOp::Xor, span),
                b"or" => self.advance_node(BinOp::Or, span),
                op => self.my_error(
                    format!("Unknown operator: '{}'", String::from_utf8_lossy(op)),
                    BinOp::Unknown,
                ),
            },
            _ => {
                // TODO is this case even reachable? Perhaps BinOp::Unknown is unnecessary
                self.my_error("expected: operator", BinOp::Unknown)
            }
        }
    }

    pub fn spanning(&mut self, from: NodeId, to: NodeId) -> (usize, usize) {
        (
            self.compiler.spans[from.0].start,
            self.compiler.spans[to.0].end,
        )
    }

    pub fn string(&mut self) -> NodeId {
        match self.tokens.peek() {
            (Token::DoubleQuotedString, span) | (Token::SingleQuotedString, span) => {
                self.advance_node(Expr::String { bareword: false }, span).id
            }
            _ => self.error("expected: string"),
        }
    }

    pub fn bareword_string(&mut self) -> ExprHandle<'a> {
        match self.tokens.peek() {
            (Token::Bareword, span) => self.advance_node(Expr::String { bareword: true }, span),
            _ => self.my_error("expected: name", Expr::Garbage),
        }
    }

    pub fn name(&mut self) -> NodeId {
        match self.tokens.peek() {
            (Token::Bareword, span) => self.advance_node(AstNode::Name, span).id,
            _ => self.error("expected: name"),
        }
    }

    pub fn call_name(&mut self) -> ExprHandle<'a> {
        let (mut token, mut span) = self.tokens.peek();

        loop {
            if [Token::Eof, Token::Newline].contains(&token) {
                break;
            }

            self.tokens.advance();
            let (next_token, next_span) = self.tokens.peek();

            if next_span.start > span.end {
                // horizontal whitespace
                break;
            }

            token = next_token;
            span.end = next_span.end;
        }

        self.create_node(Expr::String { bareword: true }, span.start, span.end)
    }

    pub fn has_tokens(&mut self) -> bool {
        self.tokens.peek_token() != Token::Eof
    }

    pub fn match_expression(&mut self) -> ExprHandle<'a> {
        let _span = span!();
        let span_start = self.position();
        let span_end;

        self.keyword(b"match");
        let target = self.simple_expression(BarewordContext::String);

        let mut match_arms = vec![];

        if !self.is_lcurly() {
            return self.my_error("expected left curly brace '{'", Expr::Garbage);
        }

        self.lcurly();

        loop {
            if self.is_rcurly() {
                span_end = self.position() + 1;
                self.rcurly();
                break;
            } else if self.is_simple_expression() {
                let pattern = self.simple_expression(BarewordContext::String);

                if !self.is_thick_arrow() {
                    return self.my_error(
                        "expected thick arrow (=>) between match cases",
                        Expr::Garbage,
                    );
                }
                self.tokens.advance();

                let pattern_result = self.simple_expression(BarewordContext::String);

                if self.is_comma() {
                    self.tokens.advance();
                }

                match_arms.push((pattern, pattern_result));
            } else if self.is_newline() {
                self.tokens.advance();
            } else {
                return self.my_error("expected match arm in match", Expr::Garbage);
            }
        }

        self.create_node(Expr::Match { target, match_arms }, span_start, span_end)
    }

    pub fn if_expression(&mut self) -> ExprHandle<'a> {
        let _span = span!();
        let span_start = self.position();
        let span_end;

        self.keyword(b"if");

        let condition = self.expression();
        self.skip_newlines();

        let then_block = self.block(BlockContext::Curlies);
        self.skip_newlines();

        let else_block = if self.is_keyword(b"else") {
            self.tokens.advance();
            self.skip_newlines();

            let block = if self.is_keyword(b"if") {
                self.if_expression()
            } else if self.is_keyword(b"match") {
                self.match_expression()
            } else {
                let block = self.block(BlockContext::Curlies);
                let span = self.compiler.get_span(block.id);
                self.create_node(Expr::Block(*block.node), span.start, span.end)
            };
            span_end = self.get_span_end(block.id);
            Some(block)
        } else {
            span_end = self.get_span_end(then_block.id);
            None
        };

        self.create_node(
            Expr::If {
                condition,
                then_block,
                else_block,
            },
            span_start,
            span_end,
        )
    }

    // directly ripped from `type_params` just changed delimiters
    // FIXME: simplify if appropriate
    pub fn signature_params(&mut self, params_context: ParamsContext) -> Handle<'a, Params<'a>> {
        let _span = span!();
        let span_start = self.position();
        let span_end;
        let param_list = {
            match params_context {
                ParamsContext::Pipes => self.pipe(),
                ParamsContext::Squares => self.lsquare(),
            }

            let mut output = vec![];

            while self.has_tokens() {
                match params_context {
                    ParamsContext::Pipes => {
                        if self.is_pipe() {
                            break;
                        }
                    }
                    ParamsContext::Squares => {
                        if self.is_rsquare() {
                            break;
                        }
                    }
                }

                if self.is_comma() {
                    self.tokens.advance();
                    continue;
                }

                let name = self.name();

                let ty = if self.is_colon() {
                    // We have a type
                    self.colon();

                    Some(self.typename())
                } else {
                    None
                };

                let name_span = self.compiler.spans[name.0];
                let param_span_end = if let Some(ty_id) = &ty {
                    self.compiler.spans[ty_id.id.0].end
                } else {
                    name_span.end
                };

                let param = self.create_node(Param { name, ty }, name_span.start, param_span_end);

                // output.push(self.name());
                output.push(param);
            }

            span_end = self.position() + 1;

            match params_context {
                ParamsContext::Pipes => self.pipe(),
                ParamsContext::Squares => self.rsquare(),
            }

            output
        };

        self.create_node(Params(param_list), span_start, span_end)
    }

    pub fn type_params(&mut self) -> Handle<'a, TypeArgs<'a>> {
        let _span = span!();
        let span_start = self.position();
        let span_end;
        let param_list = {
            self.less_than();

            let mut output = vec![];

            while self.has_tokens() {
                if self.is_greater_than() {
                    break;
                }

                if self.is_comma() {
                    self.tokens.advance();
                    continue;
                }

                output.push(self.typename());
            }

            span_end = self.position() + 1;
            self.greater_than();

            output
        };

        self.create_node(TypeArgs(param_list), span_start, span_end)
    }

    pub fn typename(&mut self) -> TypeHandle<'a> {
        let _span = span!();
        if let (Token::Bareword, span) = self.tokens.peek() {
            let name = self.name();
            let mut params = None;
            if self.is_less_than() {
                // We have generics
                params = Some(self.type_params());
            }

            let optional = if self.is_question_mark() {
                // We have an optional type
                self.tokens.advance();
                true
            } else {
                false
            };

            self.create_node(
                Type {
                    name,
                    params,
                    optional,
                },
                span.start,
                span.end,
            )
        } else {
            let garbage = self.error("expected name");
            let span = self.compiler.get_span(garbage);
            self.create_node(
                Type {
                    name: garbage,
                    params: None,
                    optional: false,
                },
                span.start,
                span.end,
            )
        }
    }

    pub fn in_out_type(&mut self) -> Handle<'a, InOutType<'a>> {
        let _span = span!();
        let span_start = self.position();

        let in_ty = self.typename();
        self.thin_arrow();
        let out_ty = self.typename();

        let span_end = self.position();
        self.create_node(InOutType(in_ty, out_ty), span_start, span_end)
    }

    pub fn in_out_types(&mut self) -> Handle<'a, InOutTypes<'a>> {
        let _span = span!();
        self.colon();

        if self.is_lsquare() {
            let span_start = self.position();

            self.tokens.advance();

            let mut output = vec![];
            while self.has_tokens() {
                if self.is_rsquare() {
                    break;
                }

                if self.is_comma() {
                    self.tokens.advance();
                    continue;
                }

                output.push(self.in_out_type());
            }

            self.rsquare();
            let span_end = self.position();

            self.create_node(InOutTypes(output), span_start, span_end)
        } else {
            let ty = self.in_out_type();
            let span = self.compiler.get_span(ty.id);
            self.create_node(InOutTypes(vec![ty]), span.start, span.end)
        }
    }

    pub fn def_statement(&mut self) -> StmtHandle<'a> {
        let _span = span!();
        let span_start = self.position();

        self.keyword(b"def");

        let name = match self.tokens.peek() {
            (Token::Bareword, span) => self.advance_node(AstNode::Name, span).id,
            (Token::DoubleQuotedString | Token::SingleQuotedString, span) => {
                self.advance_node(Expr::String { bareword: false }, span).id
            }
            _ => return self.my_error("expected def name", Stmt::Garbage),
        };

        let params = self.signature_params(ParamsContext::Squares);
        let return_ty = if self.is_colon() {
            Some(self.in_out_types())
        } else {
            None
        };
        let block = self.block(BlockContext::Curlies);

        let span_end = self.get_span_end(block.id);

        self.create_node(
            Stmt::Def(Def {
                name,
                params,
                return_ty,
                block,
            }),
            span_start,
            span_end,
        )
    }

    // TODO: Deduplicate code between let/mut/const assignments
    pub fn let_statement(&mut self) -> StmtHandle<'a> {
        let _span = span!();
        let is_mutable = false;
        let span_start = self.position();

        self.keyword(b"let");

        let variable_name = self.variable_decl();

        let ty = if self.is_colon() {
            // We have a type
            self.colon();

            Some(self.typename())
        } else {
            None
        };

        self.equals();

        let initializer = self.expression();

        let span_end = self.get_span_end(initializer.id);

        self.create_node(
            Stmt::Let {
                variable_name,
                ty,
                initializer,
                is_mutable,
            },
            span_start,
            span_end,
        )
    }

    // TODO: Deduplicate code between let/mut/const assignments
    pub fn mut_statement(&mut self) -> StmtHandle<'a> {
        let _span = span!();
        let is_mutable = true;
        let span_start = self.position();

        self.keyword(b"mut");

        let variable_name = self.variable_decl();

        let ty = if self.is_colon() {
            // We have a type
            self.colon();

            Some(self.typename())
        } else {
            None
        };

        self.equals();

        let initializer = self.expression();

        let span_end = self.get_span_end(initializer.id);

        self.create_node(
            Stmt::Let {
                variable_name,
                ty,
                initializer,
                is_mutable,
            },
            span_start,
            span_end,
        )
    }

    pub fn keyword(&mut self, keyword: &[u8]) {
        let _span = span!();
        if self.is_keyword(keyword) {
            self.tokens.advance();
        } else {
            self.error(format!(
                "expected keyword: {}",
                String::from_utf8_lossy(keyword)
            ));
        }
    }

    pub fn block(&mut self, context: BlockContext) -> BlockHandle<'a> {
        let _span = span!();
        let span_start = self.position();

        let mut code_body = vec![];
        if let BlockContext::Curlies = context {
            self.lcurly();
        }

        while self.has_tokens() {
            if self.is_rcurly() && context == BlockContext::Curlies {
                self.rcurly();
                break;
            } else if self.is_rcurly() && context == BlockContext::Closure {
                // not responsible for parsing it, yield back to the closure pass
                break;
            } else if self.is_semicolon() || self.is_newline() || self.is_comment() {
                self.tokens.advance();
                continue;
            } else if self.is_keyword(b"def") {
                code_body.push(self.def_statement());
            } else if self.is_keyword(b"let") {
                code_body.push(self.let_statement());
            } else if self.is_keyword(b"mut") {
                code_body.push(self.mut_statement());
            } else if self.is_keyword(b"while") {
                code_body.push(self.while_statement());
            } else if self.is_keyword(b"for") {
                code_body.push(self.for_statement());
            } else if self.is_keyword(b"loop") {
                code_body.push(self.loop_statement());
            } else if self.is_keyword(b"return") {
                code_body.push(self.return_statement());
            } else if self.is_keyword(b"continue") {
                code_body.push(self.continue_statement());
            } else if self.is_keyword(b"break") {
                code_body.push(self.break_statement());
            } else if self.is_keyword(b"alias") {
                code_body.push(self.alias_statement());
            } else {
                let exp_span_start = self.position();
                let expression = self.expression_or_assignment();
                let exp_span_end = self.get_span_end(expression.id);

                if self.is_semicolon() {
                    // This is a statement, not an expression
                    self.tokens.advance();
                    code_body.push(self.create_node(
                        Stmt::Expr(expression),
                        exp_span_start,
                        exp_span_end,
                    ))
                } else {
                    // TODO originally this pushed the expression directly
                    code_body.push(self.create_node(
                        Stmt::Expr(expression),
                        exp_span_start,
                        exp_span_end,
                    ));
                }
            }
        }

        self.compiler.blocks.push(Block::new(code_body));
        let span_end = self.position();

        self.create_node(
            BlockId(self.compiler.blocks.len() - 1),
            span_start,
            span_end,
        )
    }

    pub fn while_statement(&mut self) -> StmtHandle<'a> {
        let _span = span!();
        let span_start = self.position();
        self.keyword(b"while");

        if self.is_operator() {
            // TODO: flag parsing
            self.error("WIP: Flags on while are not supported yet");
            self.tokens.advance();
        }

        let condition = self.expression();
        let block = self.block(BlockContext::Curlies);
        let span_end = self.get_span_end(block.id);

        self.create_node(Stmt::While { condition, block }, span_start, span_end)
    }

    pub fn for_statement(&mut self) -> StmtHandle<'a> {
        let _span = span!();
        let span_start = self.position();
        self.keyword(b"for");

        let variable = self.variable_decl();
        self.keyword(b"in");

        let range = self.simple_expression(BarewordContext::String);
        let block = self.block(BlockContext::Curlies);
        let span_end = self.get_span_end(block.id);

        self.create_node(
            Stmt::For {
                variable,
                range,
                block,
            },
            span_start,
            span_end,
        )
    }

    pub fn loop_statement(&mut self) -> StmtHandle<'a> {
        let _span = span!();
        let span_start = self.position();
        self.keyword(b"loop");
        let block = self.block(BlockContext::Curlies);
        let span_end = self.get_span_end(block.id);

        self.create_node(Stmt::Loop { block }, span_start, span_end)
    }

    pub fn return_statement(&mut self) -> StmtHandle<'a> {
        let _span = span!();
        let span_start = self.position();
        let span_end;

        self.keyword(b"return");

        let ret_val = if self.is_expression() {
            let expr = self.expression();
            span_end = self.get_span_end(expr.id);
            Some(expr)
        } else {
            span_end = span_start + b"return".len();
            None
        };

        self.create_node(Stmt::Return(ret_val), span_start, span_end)
    }

    pub fn continue_statement(&mut self) -> StmtHandle<'a> {
        let _span = span!();
        let span_start = self.position();
        self.keyword(b"continue");
        let span_end = span_start + b"continue".len();

        self.create_node(Stmt::Continue, span_start, span_end)
    }

    pub fn break_statement(&mut self) -> StmtHandle<'a> {
        let _span = span!();
        let span_start = self.position();
        self.keyword(b"break");
        let span_end = span_start + b"break".len();

        self.create_node(Stmt::Break, span_start, span_end)
    }

    pub fn alias_statement(&mut self) -> StmtHandle<'a> {
        let _span = span!();
        let span_start = self.position();
        self.keyword(b"alias");
        let new_name = if self.is_string() {
            self.string()
        } else {
            self.name()
        };
        self.equals();
        let old_name = if self.is_string() {
            self.string()
        } else {
            self.name()
        };
        let span_end = self.get_span_end(old_name);
        self.create_node(Stmt::Alias { new_name, old_name }, span_start, span_end)
    }

    pub fn is_operator(&mut self) -> bool {
        let (token, span) = self.tokens.peek();

        match token {
            Token::Plus
            | Token::PlusPlus
            | Token::Dash
            | Token::Asterisk
            | Token::ForwardSlash
            | Token::ForwardSlashForwardSlash
            | Token::LessThan
            | Token::LessThanEqual
            | Token::GreaterThan
            | Token::GreaterThanEqual
            | Token::EqualsEquals
            | Token::ExclamationEquals
            | Token::EqualsTilde
            | Token::ExclamationTilde
            | Token::AsteriskAsterisk
            | Token::Equals
            | Token::PlusEquals
            | Token::DashEquals
            | Token::AsteriskEquals
            | Token::ForwardSlashEquals
            | Token::PlusPlusEquals => true,
            Token::Bareword => {
                let op = self.compiler.get_span_contents_manual(span.start, span.end);
                op == b"mod" || op == b"in" || op == b"and" || op == b"xor" || op == b"or"
            }
            _ => false,
        }
    }

    pub fn is_equals(&mut self) -> bool {
        self.tokens.peek_token() == Token::Equals
    }

    pub fn is_comma(&mut self) -> bool {
        self.tokens.peek_token() == Token::Comma
    }

    pub fn is_lcurly(&mut self) -> bool {
        self.tokens.peek_token() == Token::LCurly
    }

    pub fn is_rcurly(&mut self) -> bool {
        self.tokens.peek_token() == Token::RCurly
    }

    pub fn is_lparen(&mut self) -> bool {
        self.tokens.peek_token() == Token::LParen
    }

    pub fn is_rparen(&mut self) -> bool {
        self.tokens.peek_token() == Token::RParen
    }

    pub fn is_lsquare(&mut self) -> bool {
        self.tokens.peek_token() == Token::LSquare
    }

    pub fn is_rsquare(&mut self) -> bool {
        self.tokens.peek_token() == Token::RSquare
    }

    pub fn is_less_than(&mut self) -> bool {
        self.tokens.peek_token() == Token::LessThan
    }

    pub fn is_greater_than(&mut self) -> bool {
        self.tokens.peek_token() == Token::GreaterThan
    }

    pub fn is_pipe(&mut self) -> bool {
        self.tokens.peek_token() == Token::Pipe
    }

    pub fn is_dollar(&mut self) -> bool {
        self.tokens.peek_token() == Token::Dollar
    }

    pub fn is_comment(&mut self) -> bool {
        self.tokens.peek_token() == Token::Comment
    }

    pub fn is_question_mark(&mut self) -> bool {
        self.tokens.peek_token() == Token::QuestionMark
    }

    pub fn is_thin_arrow(&mut self) -> bool {
        self.tokens.peek_token() == Token::ThinArrow
    }

    pub fn is_thick_arrow(&mut self) -> bool {
        self.tokens.peek_token() == Token::ThickArrow
    }

    pub fn is_colon(&mut self) -> bool {
        self.tokens.peek_token() == Token::Colon
    }

    pub fn is_newline(&mut self) -> bool {
        self.tokens.peek_token() == Token::Newline
    }

    pub fn is_semicolon(&mut self) -> bool {
        self.tokens.peek_token() == Token::Semicolon
    }

    pub fn is_dot(&mut self) -> bool {
        self.tokens.peek_token() == Token::Dot
    }

    pub fn is_dotdot(&mut self) -> bool {
        self.tokens.peek_token() == Token::DotDot
    }

    pub fn is_coloncolon(&mut self) -> bool {
        self.tokens.peek_token() == Token::ColonColon
    }

    pub fn is_int(&mut self) -> bool {
        self.tokens.peek_token() == Token::Int
    }

    pub fn is_float(&mut self) -> bool {
        self.tokens.peek_token() == Token::Float
    }

    pub fn is_string(&mut self) -> bool {
        self.tokens.peek_token() == Token::DoubleQuotedString
            || self.tokens.peek_token() == Token::SingleQuotedString
    }

    pub fn is_keyword(&mut self, keyword: &[u8]) -> bool {
        if let (Token::Bareword, span) = self.tokens.peek() {
            self.compiler.get_span_contents_manual(span.start, span.end) == keyword
        } else {
            false
        }
    }

    pub fn is_name(&mut self) -> bool {
        self.tokens.peek_token() == Token::Bareword
    }

    pub fn is_eof(&mut self) -> bool {
        self.tokens.peek_token() == Token::Eof
    }

    pub fn is_horizontal_space(&self) -> bool {
        let span_position = self.tokens.peek_span().start;
        let whitespace: &[u8] = b" \t";

        span_position > 0 && whitespace.contains(&self.compiler.source[span_position - 1])
    }

    pub fn is_expression(&mut self) -> bool {
        self.is_simple_expression()
            || self.is_keyword(b"if")
            || self.is_keyword(b"match")
            || self.is_keyword(b"where")
    }

    pub fn is_simple_expression(&mut self) -> bool {
        self.is_string()
            || self.is_int()
            || self.is_float()
            || self.is_lcurly()
            || self.is_lsquare()
            || self.is_lparen()
            || self.is_dot()
            || self.is_dollar()
            || self.is_keyword(b"true")
            || self.is_keyword(b"false")
            || self.is_keyword(b"null")
            || self.is_name()
    }

    pub fn error_on_node(&mut self, message: impl Into<String>, node_id: NodeId) {
        self.compiler.errors.push(SourceError {
            message: message.into(),
            node_id,
            severity: Severity::Error,
        });
    }

    pub fn my_error<T: Node + 'a>(&mut self, message: impl Into<String>, node: T) -> Handle<'a, T> {
        let (token, span) = self.tokens.peek();

        if token != Token::Eof {
            self.tokens.advance();
        }

        let node = self.create_node(node, span.start, span.end);
        self.compiler.errors.push(SourceError {
            message: message.into(),
            node_id: node.id,
            severity: Severity::Error,
        });

        node
    }

    pub fn error(&mut self, message: impl Into<String>) -> NodeId {
        let (token, span) = self.tokens.peek();

        if token != Token::Eof {
            self.tokens.advance();
        }

        let node_id = self.create_node(AstNode::Garbage, span.start, span.end).id;
        self.compiler.errors.push(SourceError {
            message: message.into(),
            node_id,
            severity: Severity::Error,
        });

        node_id
    }

    pub fn create_node<T: Node + 'a>(
        &mut self,
        ast_node: T,
        span_start: usize,
        span_end: usize,
    ) -> Handle<'a, T> {
        self.compiler.push_node(
            ast_node,
            Span {
                start: span_start,
                end: span_end,
            },
        )
    }

    pub fn lparen(&mut self) {
        if self.is_lparen() {
            self.tokens.advance();
        } else {
            self.error("expected: left paren '('");
        }
    }

    pub fn rparen(&mut self) {
        if self.is_rparen() {
            self.tokens.advance();
        } else {
            self.error("expected: right paren ')'");
        }
    }

    pub fn lsquare(&mut self) {
        if self.is_lsquare() {
            self.tokens.advance();
        } else {
            self.error("expected: left bracket '['");
        }
    }

    pub fn rsquare(&mut self) {
        if self.is_rsquare() {
            self.tokens.advance();
        } else {
            self.error("expected: right bracket ']'");
        }
    }

    pub fn lcurly(&mut self) {
        if self.is_lcurly() {
            self.tokens.advance();
        } else {
            self.error("expected: left bracket '{'");
        }
    }

    pub fn rcurly(&mut self) {
        if self.is_rcurly() {
            self.tokens.advance();
        } else {
            self.error("expected: right bracket '}'");
        }
    }

    pub fn pipe(&mut self) {
        if self.is_pipe() {
            self.tokens.advance();
        } else {
            self.error("expected: pipe symbol '|'");
        }
    }

    pub fn less_than(&mut self) {
        if self.is_less_than() {
            self.tokens.advance();
        } else {
            self.error("expected: less than/left angle bracket '<'");
        }
    }

    pub fn greater_than(&mut self) {
        if self.is_greater_than() {
            self.tokens.advance();
        } else {
            self.error("expected: greater than/right angle bracket '>'");
        }
    }

    pub fn equals(&mut self) {
        if self.is_equals() {
            self.tokens.advance();
        } else {
            self.error("expected: equals '='");
        }
    }

    pub fn thin_arrow(&mut self) {
        if self.is_thin_arrow() {
            self.tokens.advance();
        } else {
            self.error("expected: thin arrow '->'");
        }
    }

    pub fn colon(&mut self) {
        if self.is_colon() {
            self.tokens.advance();
        } else {
            self.error("expected: colon ':'");
        }
    }

    pub fn comma(&mut self) {
        if self.is_comma() {
            self.tokens.advance();
        } else {
            self.error("expected: comma ','");
        }
    }

    pub fn skip_newlines(&mut self) {
        while self.is_newline() {
            self.tokens.advance();
        }
    }

    fn get_rollback_point(&self) -> RollbackPoint {
        self.compiler.get_rollback_point(self.tokens.pos())
    }

    fn apply_rollback(&mut self, rbp: RollbackPoint) {
        let token_pos = self.compiler.apply_compiler_rollback(rbp);
        self.tokens.set_pos(token_pos);
    }
}
