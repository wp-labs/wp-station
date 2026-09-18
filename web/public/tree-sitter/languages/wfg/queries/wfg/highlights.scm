; WFG Syntax Highlighting (tree-sitter native)

[
  "use"
  "scenario"
  "traffic"
  "stream"
  "gen"
  "wave"
  "burst"
  "timeline"
  "injection"
  "seq"
  "with"
  "for"
  "then"
  "not"
  "within"
  "expect"
] @keyword

[
  "hit"
  "near_miss"
  "miss"
  "precision"
  "recall"
  "fpr"
  "latency_p95"
] @keyword

[
  "base"
  "amp"
  "period"
  "shape"
  "peak"
  "every"
  "hold"
] @property

(comment) @comment
(string) @string
(number) @number
(duration) @number
(percentage) @number
(rate) @number
(boolean) @constant.builtin
(comparison_operator) @operator

[ "(" ")" "{" "}" "<" ">" ] @punctuation.bracket
[ "," "=" ] @punctuation.delimiter
"#[" @attribute
".." @operator

(scenario_declaration name: (identifier) @function.definition)
(attribute key: (identifier) @attribute)
(traffic_stream stream: (identifier) @type)
(injection_case rule: (identifier) @function)
(injection_case stream: (identifier) @type)
(seq_block entity: (identifier) @variable.parameter)
(field_predicate field: (identifier) @property)
(scenario_expect_statement rule: (identifier) @function)
(wave_rate) @constant

(identifier) @variable
