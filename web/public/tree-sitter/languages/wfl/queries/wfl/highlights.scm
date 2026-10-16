; WFL syntax highlighting

(identifier) @variable

"use" @keyword.import

[
  "pattern"
  "rule"
  "preset"
  "test"
  "scenario"
  "let"
] @keyword

[
  "if"
  "then"
  "else"
  "case"
] @keyword.control

[
  "meta"
  "events"
  "match"
  "on"
  "each"
  "where"
  "event"
  "close"
  "and"
  "join"
  "yield"
  "key"
  "conv"
  "limits"
  "for"
  "input"
  "expect"
  "options"
  "background"
  "inject"
  "replay"
  "stream"
  "gen"
  "near_miss"
  "miss"
  "use"
  "not"
  "without"
  "spread"
  "zipf"
  "as"
  "from"
  "x"
  "row"
  "tick"
  "hits"
  "hit"
  "field"
  "origin"
  "entity_type"
  "entity_id"
  "close_reason"
  "fixed"
  "within"
  "accu"
] @keyword

[
  "in"
  "not"
] @keyword.operator

[
  "snapshot"
  "asof"
  "anti"
  "session"
] @keyword.modifier

(boolean) @constant.builtin
(comparison_operator) @operator

[
  "+"
  "-"
  "!"
  "*"
  "/"
  "%"
  "&&"
  "||"
] @operator

"|" @operator
"|>" @keyword.operator
"->" @keyword.operator
"=>" @operator

[ "(" ")" "{" "}" "[" "]" ] @punctuation.bracket
[ "<" ">" ] @punctuation.bracket
[ "," ";" ":" ] @punctuation.delimiter
"." @punctuation.delimiter
"@" @punctuation.special

(comment) @comment
(string) @string
(number) @number
(duration) @number
(json_number) @number
(rate) @number
(json_null) @constant.builtin
(version_tag) @constant
(variable) @variable.special
(derive_reference) @variable.special
(close_reason_ref) @variable.builtin

(rule_declaration name: (identifier) @function.definition)
(let_declaration name: (identifier) @variable)
(pattern_declaration name: (identifier) @function.definition)
(preset_declaration name: (identifier) @type.definition)
(test_block name: (identifier) @function.definition)
(scenario_declaration name: (identifier) @function.definition)
(test_block rule: (identifier) @function)
(pattern_invocation pattern: (identifier) @function)
(case_pattern_value (identifier) @constant (#eq? @constant "_"))
"_" @constant

(event_declaration
  alias: (identifier) @variable
  window: (identifier) @type)

(match_params (field_reference) @variable.parameter)

(background_stream stream: (identifier) @type)
(inject_case rule: (identifier) @function)
(inject_case stream: (identifier) @type)
(entity_distribution window: (identifier) @type)
(entity_distribution field: (identifier) @property)
(zipf_argument key: (identifier) @property)
(join_block window: (identifier) @type)
(join_block key: (identifier) @property)
(replay_statement window: (identifier) @type)
(entity_selector field: (identifier) @property)
(file_source file: (string) @string)
(json_pair key: (json_string) @property)

(each_clause alias: (identifier) @variable)
(join_clause window: (identifier) @type)
(yield_target target: (identifier) @type)
(yield_preset_ref preset: (identifier) @type)
(entity_clause type: (identifier) @type)
(entity_clause type: (string) @type)

(transform) @keyword
(measure) @keyword
(score_call "score" @keyword)
(entity_clause "entity" @keyword)
(input_statement "row" @keyword)
(input_statement "tick" @keyword)
(object_expression "object" @keyword)
(array_expression "array" @keyword)

(function_call
  function: (identifier) @keyword)

(function_call
  object: (identifier) @type
  method: (identifier) @keyword)

(function_call function: (identifier) @function.builtin (#eq? @function.builtin "count"))
(function_call function: (identifier) @function.builtin (#eq? @function.builtin "sum"))
(function_call function: (identifier) @function.builtin (#eq? @function.builtin "avg"))
(function_call function: (identifier) @function.builtin (#eq? @function.builtin "min"))
(function_call function: (identifier) @function.builtin (#eq? @function.builtin "max"))
(function_call function: (identifier) @function.builtin (#eq? @function.builtin "distinct"))
(function_call function: (identifier) @function.builtin (#eq? @function.builtin "fmt"))
(function_call function: (identifier) @function.builtin (#eq? @function.builtin "baseline"))
(function_call function: (identifier) @function.builtin (#eq? @function.builtin "has"))
(function_call function: (identifier) @function.builtin (#eq? @function.builtin "hit"))
(function_call function: (identifier) @function.builtin (#eq? @function.builtin "contains"))
(function_call function: (identifier) @function.builtin (#eq? @function.builtin "regex_match"))
(function_call function: (identifier) @function.builtin (#eq? @function.builtin "replace"))
(function_call function: (identifier) @function.builtin (#eq? @function.builtin "replace_plain"))
(function_call function: (identifier) @function.builtin (#eq? @function.builtin "startswith"))
(function_call function: (identifier) @function.builtin (#eq? @function.builtin "startswith_any"))
(function_call function: (identifier) @function.builtin (#eq? @function.builtin "endswith"))
(function_call function: (identifier) @function.builtin (#eq? @function.builtin "endswith_any"))
(function_call function: (identifier) @function.builtin (#eq? @function.builtin "len"))
(function_call function: (identifier) @function.builtin (#eq? @function.builtin "trim"))
(function_call function: (identifier) @function.builtin (#eq? @function.builtin "ltrim"))
(function_call function: (identifier) @function.builtin (#eq? @function.builtin "rtrim"))
(function_call function: (identifier) @function.builtin (#eq? @function.builtin "lower"))
(function_call function: (identifier) @function.builtin (#eq? @function.builtin "upper"))
(function_call function: (identifier) @function.builtin (#eq? @function.builtin "substr"))
(function_call function: (identifier) @function.builtin (#eq? @function.builtin "indexof"))
(function_call function: (identifier) @function.builtin (#eq? @function.builtin "concat"))
(function_call function: (identifier) @function.builtin (#eq? @function.builtin "join"))
(function_call function: (identifier) @function.builtin (#eq? @function.builtin "join_by"))
(function_call function: (identifier) @function.builtin (#eq? @function.builtin "split"))
(function_call function: (identifier) @function.builtin (#eq? @function.builtin "time_diff"))
(function_call function: (identifier) @function.builtin (#eq? @function.builtin "time_bucket"))
(function_call function: (identifier) @function.builtin (#eq? @function.builtin "strftime"))
(function_call function: (identifier) @function.builtin (#eq? @function.builtin "strptime"))
(function_call function: (identifier) @function.builtin (#eq? @function.builtin "now"))
(function_call function: (identifier) @function.builtin (#eq? @function.builtin "now_s"))
(function_call function: (identifier) @function.builtin (#eq? @function.builtin "now_ms"))
(function_call function: (identifier) @function.builtin (#eq? @function.builtin "now_us"))
(function_call function: (identifier) @function.builtin (#eq? @function.builtin "now_ns"))
(function_call function: (identifier) @function.builtin (#eq? @function.builtin "coalesce"))
(function_call function: (identifier) @function.builtin (#eq? @function.builtin "merge"))
(function_call function: (identifier) @function.builtin (#eq? @function.builtin "isnull"))
(function_call function: (identifier) @function.builtin (#eq? @function.builtin "isnotnull"))
(function_call function: (identifier) @function.builtin (#eq? @function.builtin "is_blank"))
(function_call function: (identifier) @function.builtin (#eq? @function.builtin "null_if_blank"))
(function_call function: (identifier) @function.builtin (#eq? @function.builtin "default_if_blank"))
(function_call function: (identifier) @function.builtin (#eq? @function.builtin "md5"))
(function_call function: (identifier) @function.builtin (#eq? @function.builtin "sha1"))
(function_call function: (identifier) @function.builtin (#eq? @function.builtin "sha1_n"))
(function_call function: (identifier) @function.builtin (#eq? @function.builtin "sha256"))
(function_call function: (identifier) @function.builtin (#eq? @function.builtin "hex"))
(function_call function: (identifier) @function.builtin (#eq? @function.builtin "stable_id"))
(function_call function: (identifier) @function.builtin (#eq? @function.builtin "mvcount"))
(function_call function: (identifier) @function.builtin (#eq? @function.builtin "mvjoin"))
(function_call function: (identifier) @function.builtin (#eq? @function.builtin "mvindex"))
(function_call function: (identifier) @function.builtin (#eq? @function.builtin "mvappend"))
(function_call function: (identifier) @function.builtin (#eq? @function.builtin "mvdedup"))
(function_call function: (identifier) @function.builtin (#eq? @function.builtin "try"))
(function_call function: (identifier) @function.builtin (#eq? @function.builtin "mvsort"))
(function_call function: (identifier) @function.builtin (#eq? @function.builtin "mvreverse"))
(function_call function: (identifier) @function.builtin (#eq? @function.builtin "collect_set"))
(function_call function: (identifier) @function.builtin (#eq? @function.builtin "collect_list"))
(function_call function: (identifier) @function.builtin (#eq? @function.builtin "first"))
(function_call function: (identifier) @function.builtin (#eq? @function.builtin "last"))
(function_call function: (identifier) @function.builtin (#eq? @function.builtin "stddev"))
(function_call function: (identifier) @function.builtin (#eq? @function.builtin "percentile"))
(function_call function: (identifier) @function.builtin (#eq? @function.builtin "abs"))
(function_call function: (identifier) @function.builtin (#eq? @function.builtin "round"))
(function_call function: (identifier) @function.builtin (#eq? @function.builtin "ceil"))
(function_call function: (identifier) @function.builtin (#eq? @function.builtin "floor"))
(function_call function: (identifier) @function.builtin (#eq? @function.builtin "sqrt"))
(function_call function: (identifier) @function.builtin (#eq? @function.builtin "pow"))
(function_call function: (identifier) @function.builtin (#eq? @function.builtin "log"))
(function_call function: (identifier) @function.builtin (#eq? @function.builtin "exp"))
(function_call function: (identifier) @function.builtin (#eq? @function.builtin "clamp"))
(function_call function: (identifier) @function.builtin (#eq? @function.builtin "sign"))
(function_call function: (identifier) @function.builtin (#eq? @function.builtin "trunc"))
(function_call function: (identifier) @function.builtin (#eq? @function.builtin "is_finite"))
(function_call function: (identifier) @function.builtin (#eq? @function.builtin "external"))
(function_call function: (identifier) @function.builtin (#eq? @function.builtin "time_to_ms"))

(function_call
  object: (identifier) @type
  method: (identifier) @function.builtin
  (#eq? @function.builtin "has"))

(field_reference
  object: (identifier) @variable
  field: (identifier) @property)

(field_reference
  object: (field_reference)
  field: (identifier) @property)

(named_argument name: (yield_field (identifier) @property))
(named_argument name: (yield_field (quoted_ident) @property))
(object_item target: (object_targets (identifier) @property))
(meta_entry key: (identifier) @property)
(key_item logical: (identifier) @property)
(option_entry key: (identifier) @property)
(option_entry value: (identifier) @constant)
(limit_item value: (identifier) @constant)
(field_assignment field: (identifier) @property)
(field_assignment field: (string) @property)
(attribute key: (identifier) @property)
(field_predicate field: (identifier) @property)

[
  "max_memory"
  "max_instances"
  "max_throttle"
  "on_exceed"
 ] @property

[
  "sort"
  "top"
  "dedup"
  "where"
] @function.builtin
