; OML Syntax Highlighting Queries (default / tree-sitter native)

; ── Keywords ──
[
  "name"
  "rule"
  "enable"
  "read"
  "take"
  "pipe"
  "fmt"
  "object"
  "collect"
  "match"
  "static"
  "select"
  "from"
  "where"
  "and"
  "or"
  "not"
  "in"
  "option"
  "keys"
  "get"
] @keyword

; ── Data types ──
(data_type) @type.builtin

; ── Privacy types ──
(privacy_type) @type.builtin

; ── Built-in function names ──
[
  "Now::time"
  "Now::date"
  "Now::hour"
  "nth"
  "base64_decode"
  "path"
  "url"
  "Time::to_ts_zone"
  "starts_with"
  "map_to"
  "base64_encode"
  "html_escape"
  "html_unescape"
  "str_escape"
  "json_escape"
  "json_unescape"
  "Time::to_ts"
  "Time::to_ts_ms"
  "Time::to_ts_us"
  "to_json"
  "to_str"
  "skip_empty"
  "ip4_to_int"
  "extract_main_word"
  "extract_subject_object"
  "ends_with"
  "contains"
  "regex_match"
  "iequals"
  "is_empty"
  "gt"
  "lt"
  "eq"
  "in_range"
] @function.builtin

(pipe_fun
  "get" @function.builtin)

; SQL allows data-source-specific function names.
(sql_fun_call
  (identifier) @function)

; ── Operators ──
"|" @operator
"=>" @keyword.operator
"!" @operator
"-" @operator
"*" @operator
(sql_op) @operator

; ── Separator ──
(separator) @punctuation.special

; ── @ref ──
(at_ref) @variable.special

; ── Underscore wildcard ──
"_" @variable.builtin

; ── Boolean ──
(boolean) @constant.builtin

; ── Punctuation ──
[ "(" ")" "{" "}" "[" "]" ] @punctuation.bracket
[ "," ";" ":" "=" ] @punctuation.delimiter

; ── Strings ──
(string) @string

; ── Numbers ──
(number) @number
(ip_literal) @number

; ── Comments ──
(comment) @comment

; ── Plain identifiers (fallback; specialized captures below take precedence) ──
(identifier) @variable

; ── Target names (assignment LHS) ──
(target_name (identifier) @property)
(target_name (wild_key) @property)

; ── Static item targets ──
(static_item (target (target_name (identifier) @property)))

; ── Header name ──
(name_field name: (identifier) @type.definition)
(name_field name: (path) @type.definition)

; ── Paths ──
(path) @string.special

; ── Rule paths ──
(rule_field
  [
    (path)
    (identifier)
  ] @function)

; ── Typed values ──
(value_expr
  (identifier) @constant)

; ── JSON paths ──
(json_path) @string.special

; ── Privacy item name ──
(privacy_item name: (identifier) @property)

; ── Map targets ──
(map_targets (identifier) @property)

; ── SQL columns, comparison fields, and source tables ──
(sql_columns
  (identifier) @property)

(sql_comparison
  (identifier) @property)

(sql_expr
  "from"
  (identifier) @type)
