; WFG syntax highlighting (tree-sitter native)

; --- blocks / structural keywords ---
[
  "scenario"
  "background"
  "inject"
  "replay"
  "stream"
  "entity"
  "gen"
] @keyword

; --- inject case ---
[
  "for"
  "then"
  "use"
  "join"
  "as"
  "without"
  "within"
  "spread"
  "x"
  "from"
] @keyword

[
  "hit"
  "near_miss"
  "miss"
] @keyword

; --- rate expression ---
[
  "wave"
  "burst"
  "timeline"
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

"zipf" @function.builtin

(comment) @comment
(string) @string
(json_string) @string
(number) @number
(json_number) @number
(duration) @number
(rate) @number
(boolean) @constant.builtin
(json_null) @constant.builtin

[ "(" ")" "{" "}" "[" "]" "<" ">" ] @punctuation.bracket
[ "," "=" ":" ] @punctuation.delimiter
"#[" @attribute
".." @operator

; --- identifiers by role ---
(scenario_declaration name: (identifier) @function.definition)
(use_declaration path: (string) @string.special.path)

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
(file_source file: (string) @string.special.path)
(json_pair key: (json_string) @property)

(attribute key: (identifier) @property)
(field_predicate field: (identifier) @property)
(wave_rate) @constant

(identifier) @variable
