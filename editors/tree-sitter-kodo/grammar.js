/// <reference types="tree-sitter-cli/dsl" />
// @ts-check

const PREC = {
  RANGE: 1,
  NULL_COALESCE: 2,
  OR: 3,
  AND: 4,
  EQUALITY: 5,
  COMPARISON: 6,
  ADDITIVE: 7,
  MULTIPLICATIVE: 8,
  UNARY: 9,
  POSTFIX: 10,
  PRIMARY: 11,
};

module.exports = grammar({
  name: "kodo",

  extras: ($) => [/\s/, $.line_comment, $.block_comment],

  word: ($) => $.identifier,

  conflicts: ($) => [
    [$.struct_literal, $.block],
    [$._type, $._expression],
  ],

  rules: {
    source_file: ($) => $.module_declaration,

    // ─── Module ───────────────────────────────────────────────
    module_declaration: ($) =>
      seq(
        optional(repeat($.annotation)),
        optional(repeat($.import_statement)),
        "module",
        field("name", $.identifier),
        "{",
        optional($.meta_block),
        repeat($._module_item),
        "}"
      ),

    meta_block: ($) => seq("meta", "{", repeat($.meta_entry), "}"),

    meta_entry: ($) =>
      seq(field("key", $.meta_key), ":", field("value", $._literal), optional(",")),

    meta_key: ($) => $.identifier,

    // ─── Imports ──────────────────────────────────────────────
    import_statement: ($) =>
      choice(
        seq("import", $.import_path),
        seq("from", $.import_path, "import", commaSep1($.identifier))
      ),

    import_path: ($) => sep1($.identifier, "::"),

    // ─── Module items ─────────────────────────────────────────
    _module_item: ($) =>
      choice(
        $.function_definition,
        $.struct_definition,
        $.enum_definition,
        $.trait_definition,
        $.impl_block,
        $.actor_definition,
        $.intent_declaration,
        $.type_alias,
        $.invariant_declaration,
        $.test_block,
        $.describe_block
      ),

    // ─── Functions ────────────────────────────────────────────
    function_definition: ($) =>
      seq(
        repeat($.annotation),
        optional("pub"),
        optional("async"),
        "fn",
        field("name", $.identifier),
        optional($.type_parameters),
        "(", optional($.parameter_list), ")",
        optional(seq("->", field("return_type", $._type))),
        optional($.requires_clause),
        optional($.ensures_clause),
        $.block
      ),

    parameter_list: ($) => commaSep1($.parameter),

    parameter: ($) =>
      seq(
        optional(choice("own", "ref", seq("mut", "ref"))),
        field("name", choice($.identifier, "self")),
        optional(seq(":", field("type", $._type)))
      ),

    requires_clause: ($) => seq("requires", $.block),
    ensures_clause: ($) => seq("ensures", $.block),

    // ─── Structs ──────────────────────────────────────────────
    struct_definition: ($) =>
      seq(
        repeat($.annotation),
        optional("pub"),
        "struct",
        field("name", $.identifier),
        optional($.type_parameters),
        "{",
        optional(commaSep($.field_definition)),
        optional(","),
        "}"
      ),

    field_definition: ($) =>
      seq(
        optional("pub"),
        field("name", $.identifier),
        ":",
        field("type", $._type)
      ),

    // ─── Enums ────────────────────────────────────────────────
    enum_definition: ($) =>
      seq(
        repeat($.annotation),
        optional("pub"),
        "enum",
        field("name", $.identifier),
        optional($.type_parameters),
        "{",
        optional(commaSep($.enum_variant)),
        optional(","),
        "}"
      ),

    enum_variant: ($) =>
      seq(
        field("name", $.identifier),
        optional(seq("(", commaSep1($._type), ")"))
      ),

    // ─── Traits ───────────────────────────────────────────────
    trait_definition: ($) =>
      seq(
        repeat($.annotation),
        optional("pub"),
        "trait",
        field("name", $.identifier),
        optional($.type_parameters),
        "{",
        repeat($.trait_item),
        "}"
      ),

    trait_item: ($) =>
      choice(
        $.function_definition,
        $.associated_type
      ),

    associated_type: ($) =>
      seq("type", $.identifier, optional(seq(":", $._type)), ";"),

    // ─── Impl ─────────────────────────────────────────────────
    impl_block: ($) =>
      seq(
        "impl",
        optional($.type_parameters),
        field("trait", $._type),
        optional(seq("for", field("target", $._type))),
        "{",
        repeat(choice($.function_definition, $.associated_type_assignment)),
        "}"
      ),

    associated_type_assignment: ($) =>
      seq("type", $.identifier, "=", $._type, ";"),

    // ─── Actors ───────────────────────────────────────────────
    actor_definition: ($) =>
      seq(
        repeat($.annotation),
        optional("pub"),
        "actor",
        field("name", $.identifier),
        "{",
        repeat(choice($.field_definition, $.function_definition)),
        optional(","),
        "}"
      ),

    // ─── Intents ──────────────────────────────────────────────
    intent_declaration: ($) =>
      seq(
        repeat($.annotation),
        "intent",
        field("name", $.identifier),
        "{",
        repeat($.meta_entry),
        "}"
      ),

    // ─── Type aliases ─────────────────────────────────────────
    type_alias: ($) =>
      seq(
        optional("pub"),
        "type",
        field("name", $.identifier),
        "=",
        field("type", $._type),
        optional($.requires_clause)
      ),

    // ─── Invariants ───────────────────────────────────────────
    invariant_declaration: ($) => seq("invariant", $.block),

    // ─── Tests ────────────────────────────────────────────────
    test_block: ($) =>
      seq(
        repeat($.annotation),
        "test",
        field("name", $.string_literal),
        $.block
      ),

    describe_block: ($) =>
      seq(
        repeat($.annotation),
        "describe",
        field("name", $.string_literal),
        "{",
        optional(seq("setup", $.block)),
        optional(seq("teardown", $.block)),
        repeat(choice($.test_block, $.describe_block)),
        "}"
      ),

    // ─── Types ────────────────────────────────────────────────
    _type: ($) =>
      choice(
        $.named_type,
        $.generic_type,
        $.function_type,
        $.optional_type,
        $.tuple_type,
        $.dyn_type,
        $.unit_type
      ),

    named_type: ($) => $.identifier,

    generic_type: ($) =>
      seq($.identifier, "<", commaSep1($._type), ">"),

    function_type: ($) =>
      seq("(", optional(commaSep1($._type)), ")", "->", $._type),

    optional_type: ($) => prec(PREC.POSTFIX, seq($._type, "?")),

    tuple_type: ($) => seq("(", commaSep($._type), ")"),

    dyn_type: ($) => seq("dyn", $.identifier),

    unit_type: ($) => seq("(", ")"),

    type_parameters: ($) =>
      seq("<", commaSep1($.type_parameter), ">"),

    type_parameter: ($) =>
      seq($.identifier, optional(seq(":", $._type))),

    // ─── Statements ───────────────────────────────────────────
    block: ($) => seq("{", repeat($._statement), "}"),

    _statement: ($) =>
      choice(
        $.let_statement,
        $.return_statement,
        $.for_statement,
        $.while_statement,
        $.break_statement,
        $.continue_statement,
        $.spawn_expression,
        $.parallel_block,
        $.expression_statement
      ),

    let_statement: ($) =>
      seq(
        "let",
        optional("mut"),
        field("name", $._pattern),
        optional(seq(":", field("type", $._type))),
        optional(seq("=", field("value", $._expression)))
      ),

    return_statement: ($) => seq("return", optional($._expression)),

    for_statement: ($) =>
      seq("for", field("name", $.identifier), "in", field("iter", $._expression), $.block),

    while_statement: ($) => seq("while", field("condition", $._expression), $.block),

    break_statement: ($) => "break",
    continue_statement: ($) => "continue",

    expression_statement: ($) =>
      choice(
        seq($._expression, "=", $._expression),
        $._expression
      ),

    // ─── Expressions ──────────────────────────────────────────
    _expression: ($) =>
      choice(
        $.identifier,
        $.self_expression,
        $._literal,
        $.binary_expression,
        $.unary_expression,
        $.call_expression,
        $.method_call_expression,
        $.field_expression,
        $.index_expression,
        $.struct_literal,
        $.enum_variant_expression,
        $.if_expression,
        $.match_expression,
        $.closure_expression,
        $.try_expression,
        $.optional_chain_expression,
        $.null_coalesce_expression,
        $.await_expression,
        $.is_expression,
        $.range_expression,
        $.tuple_expression,
        $.spawn_expression,
        $.parallel_block,
        $.forall_expression,
        $.block,
        $.parenthesized_expression
      ),

    self_expression: ($) => "self",

    binary_expression: ($) =>
      choice(
        ...[
          ["+", PREC.ADDITIVE],
          ["-", PREC.ADDITIVE],
          ["*", PREC.MULTIPLICATIVE],
          ["/", PREC.MULTIPLICATIVE],
          ["%", PREC.MULTIPLICATIVE],
          ["==", PREC.EQUALITY],
          ["!=", PREC.EQUALITY],
          ["<", PREC.COMPARISON],
          [">", PREC.COMPARISON],
          ["<=", PREC.COMPARISON],
          [">=", PREC.COMPARISON],
          ["&&", PREC.AND],
          ["||", PREC.OR],
        ].map(([op, prec_val]) =>
          prec.left(
            prec_val,
            seq(field("left", $._expression), op, field("right", $._expression))
          )
        )
      ),

    unary_expression: ($) =>
      prec(PREC.UNARY, seq(choice("!", "-"), $._expression)),

    call_expression: ($) =>
      prec(
        PREC.POSTFIX,
        seq(field("function", $._expression), "(", optional(commaSep1($._expression)), ")")
      ),

    method_call_expression: ($) =>
      prec(
        PREC.POSTFIX,
        seq(
          field("object", $._expression),
          ".",
          field("method", $.identifier),
          "(",
          optional(commaSep1($._expression)),
          ")"
        )
      ),

    field_expression: ($) =>
      prec(
        PREC.POSTFIX,
        seq(field("object", $._expression), ".", field("field", choice($.identifier, $.integer_literal)))
      ),

    index_expression: ($) =>
      prec(PREC.POSTFIX, seq($._expression, "[", $._expression, "]")),

    struct_literal: ($) =>
      prec(
        PREC.PRIMARY,
        seq(
          field("name", $.identifier),
          "{",
          optional(commaSep($.field_initializer)),
          optional(","),
          "}"
        )
      ),

    field_initializer: ($) =>
      seq(field("name", $.identifier), ":", field("value", $._expression)),

    enum_variant_expression: ($) =>
      prec(
        PREC.PRIMARY,
        seq(
          field("enum", $.identifier),
          "::",
          field("variant", $.identifier),
          optional(seq("(", optional(commaSep1($._expression)), ")"))
        )
      ),

    if_expression: ($) =>
      prec.right(
        seq(
          "if",
          choice(
            field("condition", $._expression),
            seq("let", $._pattern, "=", $._expression)
          ),
          $.block,
          optional(seq("else", choice($.if_expression, $.block)))
        )
      ),

    match_expression: ($) =>
      seq("match", field("value", $._expression), "{", repeat($.match_arm), "}"),

    match_arm: ($) =>
      seq(field("pattern", $._pattern), "=>", field("body", choice($._expression, $.block)), optional(",")),

    closure_expression: ($) =>
      prec(
        PREC.PRIMARY,
        seq(
          "|",
          optional(commaSep1($.closure_parameter)),
          "|",
          optional(seq("->", $._type)),
          choice($.block, $._expression)
        )
      ),

    closure_parameter: ($) =>
      seq(field("name", $.identifier), optional(seq(":", field("type", $._type)))),

    try_expression: ($) => prec(PREC.POSTFIX, seq($._expression, "?")),

    optional_chain_expression: ($) =>
      prec(PREC.POSTFIX, seq($._expression, "?.", $.identifier)),

    null_coalesce_expression: ($) =>
      prec.left(PREC.NULL_COALESCE, seq($._expression, "??", $._expression)),

    await_expression: ($) =>
      prec(PREC.POSTFIX, seq($._expression, ".", "await")),

    is_expression: ($) =>
      prec(PREC.POSTFIX, seq($._expression, "is", $._type)),

    range_expression: ($) =>
      prec.left(PREC.RANGE, seq($._expression, choice("..", "..="), $._expression)),

    tuple_expression: ($) =>
      seq("(", commaSep1($._expression), optional(","), ")"),

    spawn_expression: ($) => seq("spawn", $.block),

    parallel_block: ($) => seq("parallel", "{", repeat($.spawn_expression), "}"),

    forall_expression: ($) =>
      seq("forall", commaSep1(seq($.identifier, ":", $._type)), $.block),

    parenthesized_expression: ($) => seq("(", $._expression, ")"),

    // ─── Patterns ─────────────────────────────────────────────
    _pattern: ($) =>
      choice(
        $.identifier,
        $.wildcard_pattern,
        $.variant_pattern,
        $.tuple_pattern,
        $._literal
      ),

    wildcard_pattern: ($) => "_",

    variant_pattern: ($) =>
      seq(
        optional(seq(field("enum", $.identifier), "::")),
        field("variant", $.identifier),
        optional(seq("(", commaSep1($._pattern), ")"))
      ),

    tuple_pattern: ($) => seq("(", commaSep($._pattern), ")"),

    // ─── Literals ─────────────────────────────────────────────
    _literal: ($) =>
      choice(
        $.integer_literal,
        $.float_literal,
        $.string_literal,
        $.fstring_literal,
        $.boolean_literal
      ),

    integer_literal: ($) => /[0-9][0-9_]*/,

    float_literal: ($) => /[0-9][0-9_]*\.[0-9][0-9_]*/,

    string_literal: ($) =>
      seq('"', repeat(choice($.escape_sequence, /[^"\\]+/)), '"'),

    fstring_literal: ($) =>
      seq('f"', repeat(choice($.escape_sequence, $.interpolation, /[^"\\{]+/)), '"'),

    interpolation: ($) => seq("{", $._expression, "}"),

    escape_sequence: ($) => /\\[nrt0"\\]/,

    boolean_literal: ($) => choice("true", "false"),

    // ─── Annotations ──────────────────────────────────────────
    annotation: ($) =>
      seq(
        "@",
        $.identifier,
        optional(seq("(", optional(commaSep1($._annotation_arg)), ")"))
      ),

    _annotation_arg: ($) =>
      choice(
        $._literal,
        $.identifier,
        seq($.identifier, ":", choice($._literal, $.identifier))
      ),

    // ─── Comments ─────────────────────────────────────────────
    line_comment: ($) => /\/\/.*/,

    block_comment: ($) => seq("/*", /[^*]*\*+([^/*][^*]*\*+)*/, "/"),

    // ─── Identifier ───────────────────────────────────────────
    identifier: ($) => /[a-zA-Z_][a-zA-Z0-9_]*/,
  },
});

function commaSep(rule) {
  return optional(commaSep1(rule));
}

function commaSep1(rule) {
  return seq(rule, repeat(seq(",", rule)));
}

function sep1(rule, separator) {
  return seq(rule, repeat(seq(separator, rule)));
}
