// C and C++ share a formatter because their tree-sitter node kinds overlap
// almost entirely. `lang_key` is "c" or "cpp" and can drive any divergences.
//
// IMPLEMENTATION STATUS: stub — parses with tree-sitter, returns source
// unchanged. The formatting logic is the main work item for this crate.
//
// GOAL: produce output identical to funky for all C/C++ inputs.
// REFERENCE: ../funky/src/formatter.rs  (token-stream reference implementation)
//            ../funky/src/config.rs      (config structure is intentionally identical)
//            ../funky/tests/corpus/      (acceptance test inputs/expected outputs)
//
// APPROACH (CST leaf-node walker):
//
//   tree-sitter does not store whitespace in the tree. Instead, walk every
//   leaf node (child_count() == 0) in source order. The "gap" between
//   consecutive leaf byte ranges is where original whitespace lived — ignore
//   it and emit formatter-controlled whitespace instead.
//
//   For each leaf, determine what whitespace to emit *before* it based on:
//     - the previous leaf's node kind
//     - the current leaf's node kind
//     - the current node's parent/ancestor chain (for indentation depth)
//     - the active Config
//
//   Context that a plain token-stream walker cannot derive from a flat stream
//   is now cheaply available via node.parent() and ancestor iteration — use
//   that to simplify logic compared to funky's BraceCtx stack.
//
// KEY TREE-SITTER NODE KINDS (C / C++):
//   translation_unit        root of every file
//   function_definition     function definition (not declaration)
//   compound_statement      { ... } block
//   if_statement            if / else if / else
//   for_statement           for loop
//   while_statement         while loop
//   do_statement            do { } while
//   switch_statement        switch
//   case_statement          case / default arm
//   return_statement        return expr;
//   declaration             variable or type declaration
//   type_definition         typedef
//   struct_specifier        struct { }
//   union_specifier         union { }
//   enum_specifier          enum { }
//   comment                 // line comment OR /* block comment */
//   preproc_include         #include
//   preproc_def             #define (object-like)
//   preproc_function_def    #define (function-like)
//   preproc_if              #if / #ifdef / #ifndef
//   preproc_else            #else
//   preproc_elif            #elif
//   preproc_endif           #endif (unnamed, anon node)
//   parameter_declaration   function parameter
//   abstract_declarator     e.g. pointer in a cast
//   initializer_list        { a, b, c } aggregate init
//   "("  ")"  "{"  "}"     anonymous punctuation nodes
//   "["  "]"  "<"  ">"
//   ";"  ","  ":"
//   "="  "+"  "-"  "*"  "/", etc.  (binary op anonymous nodes)
//
// NOTE: anonymous nodes (node.is_named() == false) are punctuation and
//   operators. Named nodes are syntactic constructs. Use node.kind() for both.

use crate::config::Config;
use crate::error::MoldyError;

pub fn format(source: &str, lang_key: &str, config: &Config) -> Result<String, MoldyError> {
    let ts_lang: tree_sitter::Language = match lang_key {
        "c"   => lang_parsing_substrate::tree_sitter_c::LANGUAGE.into(),
        "cpp" => lang_parsing_substrate::tree_sitter_cpp::LANGUAGE.into(),
        _     => unreachable!("c_cpp formatter called with unexpected language key"),
    };

    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&ts_lang)
        .map_err(|e| MoldyError::Parse(e.to_string()))?;

    let tree = parser
        .parse(source, None)
        .ok_or_else(|| MoldyError::Parse("tree-sitter returned no tree".into()))?;

    if tree.root_node().has_error() {
        // Pass source through unchanged when the file has syntax errors.
        // TODO: decide whether to attempt partial formatting.
        return Ok(source.to_string());
    }

    // ── TODO: implement CST-based formatter ──────────────────────────────────
    //
    // Replace this stub with a Formatter struct (see below for the skeleton)
    // that walks all leaf nodes in order and reconstructs formatted output.
    //
    // Suggested struct layout:
    //
    //   struct Fmt<'src> {
    //       src:    &'src str,
    //       config: &'src Config,
    //       out:    String,
    //       depth:  u32,          // current brace depth
    //       pp_depth: u32,        // preprocessor #if nesting depth
    //   }
    //
    //   impl<'src> Fmt<'src> {
    //       fn run(mut self, tree: &tree_sitter::Tree) -> Result<String, MoldyError> {
    //           let mut leaves = vec![];
    //           collect_leaves(tree.root_node(), &mut leaves);
    //           for (i, node) in leaves.iter().enumerate() {
    //               let prev = if i > 0 { Some(leaves[i-1]) } else { None };
    //               self.emit_before(prev, *node);
    //               self.out.push_str(&self.src[node.start_byte()..node.end_byte()]);
    //           }
    //           Ok(self.out)
    //       }
    //   }
    //
    //   fn collect_leaves<'tree>(node: tree_sitter::Node<'tree>, out: &mut Vec<tree_sitter::Node<'tree>>) {
    //       if node.child_count() == 0 { out.push(node); return; }
    //       let mut cursor = node.walk();
    //       for child in node.children(&mut cursor) { collect_leaves(child, out); }
    //   }

    let _ = (tree, config);
    Ok(source.to_string())
}
