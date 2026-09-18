; WFL syntax highlighting

(identifier) @variable

"use" @keyword.import

[
  "pattern"
  "rule"
  "preset"
  "test"
  "scenario"
] @keyword

[
  "if"
  "then"
  "else"
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
  "traffic"
  "stream"
  "gen"
  "injection"
  "near_miss"
  "miss"
  "precision"
  "recall"
  "fpr"
  "latency_p95"
  "seq"
  "use"
  "not"
  "hits"
  "hit"
  "field"
  "origin"
  "entity_type"
  "entity_id"
  "close_reason"
  "fixed"
  "within"
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
  "*"
  "/"
  "%"
  "&&"
  "||"
] @operator

"|" @operator
"|>" @keyword.operator
"->" @keyword.operator

[ "(" ")" "{" "}" "[" "]" ] @punctuation.bracket
[ "<" ">" ] @punctuation.bracket
[ "," ";" ":" ] @punctuation.delimiter
"." @punctuation.delimiter
"@" @punctuation.special

(comment) @comment
(string) @string
(number) @number
(duration) @number
(percentage) @number
(rate) @number
(version_tag) @constant
(variable) @variable.special
(derive_reference) @variable.special
(close_reason_ref) @variable.builtin

(rule_declaration name: (identifier) @function.definition)
(pattern_declaration name: (identifier) @function.definition)
(yield_preset_declaration name: (identifier) @type.definition)
(test_block name: (identifier) @function.definition)
(scenario_declaration name: (identifier) @function.definition)
(test_block rule: (identifier) @function)
(pattern_invocation pattern: (identifier) @function)

(event_declaration
  alias: (identifier) @variable
  window: (identifier) @type)

(match_params (field_reference) @variable.parameter)

(traffic_stream stream: (identifier) @type)
(injection_case rule: (identifier) @function)
(injection_case stream: (identifier) @type)
(seq_block entity: (identifier) @variable)
(scenario_expect_statement rule: (identifier) @function)

(each_clause alias: (identifier) @variable)
(join_clause window: (identifier) @type)
(yield_target target: (identifier) @type)
(yield_clause preset: (preset_ref base: (identifier) @type))
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

(function_call
  object: (identifier) @type
  method: (identifier) @function.builtin
  (#eq? @function.builtin "has"))

(field_reference
  object: (identifier) @variable
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
