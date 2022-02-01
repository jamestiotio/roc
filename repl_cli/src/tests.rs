use indoc::indoc;
use std::env;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, ExitStatus, Stdio};

use roc_test_utils::assert_multiline_str_eq;

use crate::{INSTRUCTIONS, WELCOME_MESSAGE};

const ERROR_MESSAGE_START: char = '─';

#[derive(Debug)]
pub struct Out {
    pub stdout: String,
    pub stderr: String,
    pub status: ExitStatus,
}

pub fn path_to_roc_binary() -> PathBuf {
    // Adapted from https://github.com/volta-cli/volta/blob/cefdf7436a15af3ce3a38b8fe53bb0cfdb37d3dd/tests/acceptance/support/sandbox.rs#L680
    // by the Volta Contributors - license information can be found in
    // the LEGAL_DETAILS file in the root directory of this distribution.
    //
    // Thank you, Volta contributors!
    let mut path = env::var_os("CARGO_BIN_PATH")
            .map(PathBuf::from)
            .or_else(|| {
                env::current_exe().ok().map(|mut path| {
                    path.pop();
                    if path.ends_with("deps") {
                        path.pop();
                    }
                    path
                })
            })
            .unwrap_or_else(|| panic!("CARGO_BIN_PATH wasn't set, and couldn't be inferred from context. Can't run CLI tests."));

    path.push("roc");

    path
}

fn repl_eval(input: &str) -> Out {
    let mut cmd = Command::new(path_to_roc_binary());

    cmd.arg("repl");

    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("failed to execute compiled `roc` binary in CLI test");

    {
        let stdin = child.stdin.as_mut().expect("Failed to open stdin");

        // Send the input expression
        stdin
            .write_all(input.as_bytes())
            .expect("Failed to write input to stdin");

        // Evaluate the expression
        stdin
            .write_all(b"\n")
            .expect("Failed to write newline to stdin");

        // Gracefully exit the repl
        stdin
            .write_all(b":exit\n")
            .expect("Failed to write :exit to stdin");
    }

    let output = child
        .wait_with_output()
        .expect("Error waiting for REPL child process to exit.");

    // Remove the initial instructions from the output.

    let expected_instructions = format!("{}{}", WELCOME_MESSAGE, INSTRUCTIONS);
    let stdout = String::from_utf8(output.stdout).unwrap();

    assert!(
        stdout.starts_with(&expected_instructions),
        "Unexpected repl output: {}",
        stdout
    );

    let (_, answer) = stdout.split_at(expected_instructions.len());
    let answer = if answer.is_empty() {
        // The repl crashed before completing the evaluation.
        // This is most likely due to a segfault.
        if output.status.to_string() == "signal: 11" {
            panic!(
                "repl segfaulted during the test. Stderr was {:?}",
                String::from_utf8(output.stderr).unwrap()
            );
        } else {
            panic!("repl exited unexpectedly before finishing evaluation. Exit status was {:?} and stderr was {:?}", output.status, String::from_utf8(output.stderr).unwrap());
        }
    } else {
        let expected_after_answer = "\n".to_string();

        assert!(
            answer.ends_with(&expected_after_answer),
            "Unexpected repl output after answer: {}",
            answer
        );

        // Use [1..] to trim the leading '\n'
        // and (len - 1) to trim the trailing '\n'
        let (answer, _) = answer[1..].split_at(answer.len() - expected_after_answer.len() - 1);

        // Remove ANSI escape codes from the answer - for example:
        //
        //     Before: "42 \u{1b}[35m:\u{1b}[0m Num *"
        //     After:  "42 : Num *"
        strip_ansi_escapes::strip(answer).unwrap()
    };

    Out {
        stdout: String::from_utf8(answer).unwrap(),
        stderr: String::from_utf8(output.stderr).unwrap(),
        status: output.status,
    }
}

fn expect_success(input: &str, expected: &str) {
    let out = repl_eval(input);

    assert_multiline_str_eq!("", out.stderr.as_str());
    assert_multiline_str_eq!(expected, out.stdout.as_str());
    assert!(out.status.success());
}

fn expect_failure(input: &str, expected: &str) {
    let out = repl_eval(input);

    // there may be some other stuff printed (e.g. unification errors)
    // so skip till the header of the first error
    match out.stdout.find(ERROR_MESSAGE_START) {
        Some(index) => {
            assert_multiline_str_eq!("", out.stderr.as_str());
            assert_multiline_str_eq!(expected, &out.stdout[index..]);
            assert!(out.status.success());
        }
        None => {
            assert_multiline_str_eq!("", out.stderr.as_str());
            assert!(out.status.success());
            panic!(
                "I expected a failure, but there is no error message in stdout:\n\n{}",
                &out.stdout
            );
        }
    }
}

#[test]
fn literal_0() {
    expect_success("0", "0 : Num *");
}

#[test]
fn literal_42() {
    expect_success("42", "42 : Num *");
}

#[test]
fn literal_0x0() {
    expect_success("0x0", "0 : Int *");
}

#[test]
fn literal_0x42() {
    expect_success("0x42", "66 : Int *");
}

#[test]
fn literal_0point0() {
    expect_success("0.0", "0 : Float *");
}

#[test]
fn literal_4point2() {
    expect_success("4.2", "4.2 : Float *");
}

#[test]
fn num_addition() {
    expect_success("1 + 2", "3 : Num *");
}

#[test]
fn int_addition() {
    expect_success("0x1 + 2", "3 : I64");
}

#[test]
fn float_addition() {
    expect_success("1.1 + 2", "3.1 : F64");
}

#[test]
fn num_rem() {
    expect_success("299 % 10", "Ok 9 : Result (Int *) [ DivByZero ]*");
}

#[test]
fn num_floor_division_success() {
    expect_success("Num.divFloor 4 3", "Ok 1 : Result (Int *) [ DivByZero ]*");
}

#[test]
fn num_floor_division_divby_zero() {
    expect_success(
        "Num.divFloor 4 0",
        "Err DivByZero : Result (Int *) [ DivByZero ]*",
    );
}

#[test]
fn num_ceil_division_success() {
    expect_success("Num.divCeil 4 3", "Ok 2 : Result (Int *) [ DivByZero ]*")
}

#[test]
fn bool_in_record() {
    expect_success("{ x: 1 == 1 }", "{ x: True } : { x : Bool }");
    expect_success(
        "{ z: { y: { x: 1 == 1 } } }",
        "{ z: { y: { x: True } } } : { z : { y : { x : Bool } } }",
    );
    expect_success("{ x: 1 != 1 }", "{ x: False } : { x : Bool }");
    expect_success(
        "{ x: 1 == 1, y: 1 != 1 }",
        "{ x: True, y: False } : { x : Bool, y : Bool }",
    );
}

#[test]
fn bool_basic_equality() {
    expect_success("1 == 1", "True : Bool");
    expect_success("1 != 1", "False : Bool");
}

#[test]
fn arbitrary_tag_unions() {
    expect_success("if 1 == 1 then Red else Green", "Red : [ Green, Red ]*");
    expect_success("if 1 != 1 then Red else Green", "Green : [ Green, Red ]*");
}

#[test]
fn tag_without_arguments() {
    expect_success("True", "True : [ True ]*");
    expect_success("False", "False : [ False ]*");
}

#[test]
fn byte_tag_union() {
    expect_success(
        "if 1 == 1 then Red else if 1 == 1 then Green else Blue",
        "Red : [ Blue, Green, Red ]*",
    );

    expect_success(
        "{ y: { x: if 1 == 1 then Red else if 1 == 1 then Green else Blue } }",
        "{ y: { x: Red } } : { y : { x : [ Blue, Green, Red ]* } }",
    );
}

#[test]
fn tag_in_record() {
    expect_success(
        "{ x: Foo 1 2 3, y : 4 }",
        "{ x: Foo 1 2 3, y: 4 } : { x : [ Foo (Num *) (Num *) (Num *) ]*, y : Num * }",
    );
    expect_success(
        "{ x: Foo 1 2 3 }",
        "{ x: Foo 1 2 3 } : { x : [ Foo (Num *) (Num *) (Num *) ]* }",
    );
    expect_success("{ x: Unit }", "{ x: Unit } : { x : [ Unit ]* }");
}

#[test]
fn single_element_tag_union() {
    expect_success("True 1", "True 1 : [ True (Num *) ]*");
    expect_success("Foo 1 3.14", "Foo 1 3.14 : [ Foo (Num *) (Float *) ]*");
}

#[test]
fn newtype_of_unit() {
    expect_success("Foo Bar", "Foo Bar : [ Foo [ Bar ]* ]*");
}

#[test]
fn newtype_of_big_data() {
    expect_success(
        indoc!(
            r#"
                Either a b : [ Left a, Right b ]
                lefty : Either Str Str
                lefty = Left "loosey"
                A lefty
                "#
        ),
        r#"A (Left "loosey") : [ A (Either Str Str) ]*"#,
    )
}

#[test]
fn newtype_nested() {
    expect_success(
        indoc!(
            r#"
                Either a b : [ Left a, Right b ]
                lefty : Either Str Str
                lefty = Left "loosey"
                A (B (C lefty))
                "#
        ),
        r#"A (B (C (Left "loosey"))) : [ A [ B [ C (Either Str Str) ]* ]* ]*"#,
    )
}

#[test]
fn newtype_of_big_of_newtype() {
    expect_success(
        indoc!(
            r#"
                Big a : [ Big a [ Wrapper [ Newtype a ] ] ]
                big : Big Str
                big = Big "s" (Wrapper (Newtype "t"))
                A big
                "#
        ),
        r#"A (Big "s" (Wrapper (Newtype "t"))) : [ A (Big Str) ]*"#,
    )
}

#[test]
fn tag_with_arguments() {
    expect_success("True 1", "True 1 : [ True (Num *) ]*");

    expect_success(
        "if 1 == 1 then True 3 else False 3.14",
        "True 3 : [ False (Float *), True (Num *) ]*",
    )
}

#[test]
fn literal_empty_str() {
    expect_success("\"\"", "\"\" : Str");
}

#[test]
fn literal_ascii_str() {
    expect_success("\"Hello, World!\"", "\"Hello, World!\" : Str");
}

#[test]
fn literal_utf8_str() {
    expect_success("\"👩‍👩‍👦‍👦\"", "\"👩‍👩‍👦‍👦\" : Str");
}

#[test]
fn str_concat() {
    expect_success(
        "Str.concat \"Hello, \" \"World!\"",
        "\"Hello, World!\" : Str",
    );
}

#[test]
fn str_count_graphemes() {
    expect_success("Str.countGraphemes \"å🤔\"", "2 : Nat");
}

#[test]
fn literal_empty_list() {
    expect_success("[]", "[] : List *");
}

#[test]
fn literal_empty_list_empty_record() {
    expect_success("[ {} ]", "[ {} ] : List {}");
}

#[test]
fn literal_num_list() {
    expect_success("[ 1, 2, 3 ]", "[ 1, 2, 3 ] : List (Num *)");
}

#[test]
fn literal_int_list() {
    expect_success("[ 0x1, 0x2, 0x3 ]", "[ 1, 2, 3 ] : List (Int *)");
}

#[test]
fn literal_float_list() {
    expect_success("[ 1.1, 2.2, 3.3 ]", "[ 1.1, 2.2, 3.3 ] : List (Float *)");
}

#[test]
fn literal_string_list() {
    expect_success(r#"[ "a", "b", "cd" ]"#, r#"[ "a", "b", "cd" ] : List Str"#);
}

#[test]
fn nested_string_list() {
    expect_success(
        r#"[ [ [ "a", "b", "cd" ], [ "y", "z" ] ], [ [] ], [] ]"#,
        r#"[ [ [ "a", "b", "cd" ], [ "y", "z" ] ], [ [] ], [] ] : List (List (List Str))"#,
    );
}

#[test]
fn nested_num_list() {
    expect_success(
        r#"[ [ [ 4, 3, 2 ], [ 1, 0 ] ], [ [] ], [] ]"#,
        r#"[ [ [ 4, 3, 2 ], [ 1, 0 ] ], [ [] ], [] ] : List (List (List (Num *)))"#,
    );
}

#[test]
fn nested_int_list() {
    expect_success(
        r#"[ [ [ 4, 3, 2 ], [ 1, 0x0 ] ], [ [] ], [] ]"#,
        r#"[ [ [ 4, 3, 2 ], [ 1, 0 ] ], [ [] ], [] ] : List (List (List I64))"#,
    );
}

#[test]
fn nested_float_list() {
    expect_success(
        r#"[ [ [ 4, 3, 2 ], [ 1, 0.0 ] ], [ [] ], [] ]"#,
        r#"[ [ [ 4, 3, 2 ], [ 1, 0 ] ], [ [] ], [] ] : List (List (List F64))"#,
    );
}

#[test]
fn num_bitwise_and() {
    expect_success("Num.bitwiseAnd 20 20", "20 : Int *");

    expect_success("Num.bitwiseAnd 25 10", "8 : Int *");

    expect_success("Num.bitwiseAnd 200 0", "0 : Int *")
}

#[test]
fn num_bitwise_xor() {
    expect_success("Num.bitwiseXor 20 20", "0 : Int *");

    expect_success("Num.bitwiseXor 15 14", "1 : Int *");

    expect_success("Num.bitwiseXor 7 15", "8 : Int *");

    expect_success("Num.bitwiseXor 200 0", "200 : Int *")
}

#[test]
fn num_add_wrap() {
    expect_success(
        "Num.addWrap Num.maxI64 1",
        "-9223372036854775808 : Int Signed64",
    );
}

#[test]
fn num_sub_wrap() {
    expect_success(
        "Num.subWrap Num.minI64 1",
        "9223372036854775807 : Int Signed64",
    );
}

#[test]
fn num_mul_wrap() {
    expect_success("Num.mulWrap Num.maxI64 2", "-2 : Int Signed64");
}

#[test]
fn num_add_checked() {
    expect_success("Num.addChecked 1 1", "Ok 2 : Result (Num *) [ Overflow ]*");
    expect_success(
        "Num.addChecked Num.maxI64 1",
        "Err Overflow : Result I64 [ Overflow ]*",
    );
}

#[test]
fn num_sub_checked() {
    expect_success("Num.subChecked 1 1", "Ok 0 : Result (Num *) [ Overflow ]*");
    expect_success(
        "Num.subChecked Num.minI64 1",
        "Err Overflow : Result I64 [ Overflow ]*",
    );
}

#[test]
fn num_mul_checked() {
    expect_success(
        "Num.mulChecked 20 2",
        "Ok 40 : Result (Num *) [ Overflow ]*",
    );
    expect_success(
        "Num.mulChecked Num.maxI64 2",
        "Err Overflow : Result I64 [ Overflow ]*",
    );
}

#[test]
fn list_concat() {
    expect_success(
        "List.concat [ 1.1, 2.2 ] [ 3.3, 4.4, 5.5 ]",
        "[ 1.1, 2.2, 3.3, 4.4, 5.5 ] : List (Float *)",
    );
}

#[test]
fn list_contains() {
    expect_success("List.contains [] 0", "False : Bool");
    expect_success("List.contains [ 1, 2, 3 ] 2", "True : Bool");
    expect_success("List.contains [ 1, 2, 3 ] 4", "False : Bool");
}

#[test]
fn list_sum() {
    expect_success("List.sum []", "0 : Num *");
    expect_success("List.sum [ 1, 2, 3 ]", "6 : Num *");
    expect_success("List.sum [ 1.1, 2.2, 3.3 ]", "6.6 : F64");
}

#[test]
fn list_first() {
    expect_success(
        "List.first [ 12, 9, 6, 3 ]",
        "Ok 12 : Result (Num *) [ ListWasEmpty ]*",
    );
    expect_success(
        "List.first []",
        "Err ListWasEmpty : Result * [ ListWasEmpty ]*",
    );
}

#[test]
fn list_last() {
    expect_success(
        "List.last [ 12, 9, 6, 3 ]",
        "Ok 3 : Result (Num *) [ ListWasEmpty ]*",
    );

    expect_success(
        "List.last []",
        "Err ListWasEmpty : Result * [ ListWasEmpty ]*",
    );
}

#[test]
fn empty_record() {
    expect_success("{}", "{} : {}");
}

#[test]
fn basic_1_field_i64_record() {
    // Even though this gets unwrapped at runtime, the repl should still
    // report it as a record
    expect_success("{ foo: 42 }", "{ foo: 42 } : { foo : Num * }");
}

#[test]
fn basic_1_field_f64_record() {
    // Even though this gets unwrapped at runtime, the repl should still
    // report it as a record
    expect_success("{ foo: 4.2 }", "{ foo: 4.2 } : { foo : Float * }");
}

#[test]
fn nested_1_field_i64_record() {
    // Even though this gets unwrapped at runtime, the repl should still
    // report it as a record
    expect_success(
        "{ foo: { bar: { baz: 42 } } }",
        "{ foo: { bar: { baz: 42 } } } : { foo : { bar : { baz : Num * } } }",
    );
}

#[test]
fn nested_1_field_f64_record() {
    // Even though this gets unwrapped at runtime, the repl should still
    // report it as a record
    expect_success(
        "{ foo: { bar: { baz: 4.2 } } }",
        "{ foo: { bar: { baz: 4.2 } } } : { foo : { bar : { baz : Float * } } }",
    );
}

#[test]
fn basic_2_field_i64_record() {
    expect_success(
        "{ foo: 0x4, bar: 0x2 }",
        "{ bar: 2, foo: 4 } : { bar : Int *, foo : Int * }",
    );
}

#[test]
fn basic_2_field_f64_record() {
    expect_success(
        "{ foo: 4.1, bar: 2.3 }",
        "{ bar: 2.3, foo: 4.1 } : { bar : Float *, foo : Float * }",
    );
}

#[test]
fn basic_2_field_mixed_record() {
    expect_success(
        "{ foo: 4.1, bar: 2 }",
        "{ bar: 2, foo: 4.1 } : { bar : Num *, foo : Float * }",
    );
}

#[test]
fn basic_3_field_record() {
    expect_success(
        "{ foo: 4.1, bar: 2, baz: 0x5 }",
        "{ bar: 2, baz: 5, foo: 4.1 } : { bar : Num *, baz : Int *, foo : Float * }",
    );
}

#[test]
fn list_of_1_field_records() {
    // Even though these get unwrapped at runtime, the repl should still
    // report them as records
    expect_success("[ { foo: 42 } ]", "[ { foo: 42 } ] : List { foo : Num * }");
}

#[test]
fn list_of_2_field_records() {
    expect_success(
        "[ { foo: 4.1, bar: 2 } ]",
        "[ { bar: 2, foo: 4.1 } ] : List { bar : Num *, foo : Float * }",
    );
}

#[test]
fn three_element_record() {
    // if this tests turns out to fail on 32-bit platforms, look at jit_to_ast_help
    expect_success(
        "{ a: 1, b: 2, c: 3 }",
        "{ a: 1, b: 2, c: 3 } : { a : Num *, b : Num *, c : Num * }",
    );
}

#[test]
fn four_element_record() {
    // if this tests turns out to fail on 32-bit platforms, look at jit_to_ast_help
    expect_success(
        "{ a: 1, b: 2, c: 3, d: 4 }",
        "{ a: 1, b: 2, c: 3, d: 4 } : { a : Num *, b : Num *, c : Num *, d : Num * }",
    );
}

// #[test]
// fn multiline_string() {
//     // If a string contains newlines, format it as a multiline string in the output
//     expect_success(r#""\n\nhi!\n\n""#, "\"\"\"\n\nhi!\n\n\"\"\"");
// }

#[test]
fn list_of_3_field_records() {
    expect_success(
        "[ { foo: 4.1, bar: 2, baz: 0x3 } ]",
        "[ { bar: 2, baz: 3, foo: 4.1 } ] : List { bar : Num *, baz : Int *, foo : Float * }",
    );
}

#[test]
fn identity_lambda() {
    expect_success("\\x -> x", "<function> : a -> a");
}

#[test]
fn sum_lambda() {
    expect_success("\\x, y -> x + y", "<function> : Num a, Num a -> Num a");
}

#[test]
fn stdlib_function() {
    expect_success("Num.abs", "<function> : Num a -> Num a");
}

#[test]
fn too_few_args() {
    expect_failure(
        "Num.add 2",
        indoc!(
            r#"
                ── TOO FEW ARGS ────────────────────────────────────────────────────────────────

                The add function expects 2 arguments, but it got only 1:

                4│      Num.add 2
                        ^^^^^^^

                Roc does not allow functions to be partially applied. Use a closure to
                make partial application explicit.
                "#
        ),
    );
}

#[test]
fn type_problem() {
    expect_failure(
        "1 + \"\"",
        indoc!(
            r#"
                ── TYPE MISMATCH ───────────────────────────────────────────────────────────────

                The 2nd argument to add is not what I expect:

                4│      1 + ""
                            ^^

                This argument is a string of type:

                    Str

                But add needs the 2nd argument to be:

                    Num a
                "#
        ),
    );
}

#[test]
fn issue_2149() {
    expect_success(r#"Str.toI8 "127""#, "Ok 127 : Result I8 [ InvalidNumStr ]*");
    expect_success(
        r#"Str.toI8 "128""#,
        "Err InvalidNumStr : Result I8 [ InvalidNumStr ]*",
    );
    expect_success(
        r#"Str.toI16 "32767""#,
        "Ok 32767 : Result I16 [ InvalidNumStr ]*",
    );
    expect_success(
        r#"Str.toI16 "32768""#,
        "Err InvalidNumStr : Result I16 [ InvalidNumStr ]*",
    );
}

#[test]
fn multiline_input() {
    expect_success(
        indoc!(
            r#"
                a : Str
                a = "123"
                a
                "#
        ),
        r#""123" : Str"#,
    )
}

#[test]
fn recursive_tag_union_flat_variant() {
    expect_success(
        indoc!(
            r#"
                Expr : [ Sym Str, Add Expr Expr ]
                s : Expr
                s = Sym "levitating"
                s
                "#
        ),
        r#"Sym "levitating" : Expr"#,
    )
}

#[test]
fn large_recursive_tag_union_flat_variant() {
    expect_success(
        // > 7 variants so that to force tag storage alongside the data
        indoc!(
            r#"
                Item : [ A Str, B Str, C Str, D Str, E Str, F Str, G Str, H Str, I Str, J Str, K Item ]
                s : Item
                s = H "woo"
                s
                "#
        ),
        r#"H "woo" : Item"#,
    )
}

#[test]
fn recursive_tag_union_recursive_variant() {
    expect_success(
        indoc!(
            r#"
                Expr : [ Sym Str, Add Expr Expr ]
                s : Expr
                s = Add (Add (Sym "one") (Sym "two")) (Sym "four")
                s
                "#
        ),
        r#"Add (Add (Sym "one") (Sym "two")) (Sym "four") : Expr"#,
    )
}

#[test]
fn large_recursive_tag_union_recursive_variant() {
    expect_success(
        // > 7 variants so that to force tag storage alongside the data
        indoc!(
            r#"
                Item : [ A Str, B Str, C Str, D Str, E Str, F Str, G Str, H Str, I Str, J Str, K Item, L Item ]
                s : Item
                s = K (L (E "woo"))
                s
                "#
        ),
        r#"K (L (E "woo")) : Item"#,
    )
}

#[test]
fn recursive_tag_union_into_flat_tag_union() {
    expect_success(
        indoc!(
            r#"
                Item : [ One [ A Str, B Str ], Deep Item ]
                i : Item
                i = Deep (One (A "woo"))
                i
                "#
        ),
        r#"Deep (One (A "woo")) : Item"#,
    )
}

#[test]
fn non_nullable_unwrapped_tag_union() {
    expect_success(
        indoc!(
            r#"
                RoseTree a : [ Tree a (List (RoseTree a)) ]
                e1 : RoseTree Str
                e1 = Tree "e1" []
                e2 : RoseTree Str
                e2 = Tree "e2" []
                combo : RoseTree Str
                combo = Tree "combo" [e1, e2]
                combo
                "#
        ),
        r#"Tree "combo" [ Tree "e1" [], Tree "e2" [] ] : RoseTree Str"#,
    )
}

#[test]
fn nullable_unwrapped_tag_union() {
    expect_success(
        indoc!(
            r#"
                LinkedList a : [ Nil, Cons a (LinkedList a) ]
                c1 : LinkedList Str
                c1 = Cons "Red" Nil
                c2 : LinkedList Str
                c2 = Cons "Yellow" c1
                c3 : LinkedList Str
                c3 = Cons "Green" c2
                c3
                "#
        ),
        r#"Cons "Green" (Cons "Yellow" (Cons "Red" Nil)) : LinkedList Str"#,
    )
}

#[test]
fn nullable_wrapped_tag_union() {
    expect_success(
        indoc!(
            r#"
                Container a : [ Empty, Whole a, Halved (Container a) (Container a) ]

                meats : Container Str
                meats = Halved (Whole "Brisket") (Whole "Ribs")

                sides : Container Str
                sides = Halved (Whole "Coleslaw") Empty

                bbqPlate : Container Str
                bbqPlate = Halved meats sides

                bbqPlate
                "#
        ),
        r#"Halved (Halved (Whole "Brisket") (Whole "Ribs")) (Halved (Whole "Coleslaw") Empty) : Container Str"#,
    )
}

#[test]
fn large_nullable_wrapped_tag_union() {
    // > 7 non-empty variants so that to force tag storage alongside the data
    expect_success(
        indoc!(
            r#"
                Cont a : [ Empty, S1 a, S2 a, S3 a, S4 a, S5 a, S6 a, S7 a, Tup (Cont a) (Cont a) ]

                fst : Cont Str
                fst = Tup (S1 "S1") (S2 "S2")

                snd : Cont Str
                snd = Tup (S5 "S5") Empty

                tup : Cont Str
                tup = Tup fst snd

                tup
                "#
        ),
        r#"Tup (Tup (S1 "S1") (S2 "S2")) (Tup (S5 "S5") Empty) : Cont Str"#,
    )
}

#[test]
fn issue_2300() {
    expect_success(
        r#"\Email str -> str == """#,
        r#"<function> : [ Email Str ] -> Bool"#,
    )
}

#[test]
fn function_in_list() {
    expect_success(
        r#"[\x -> x + 1, \s -> s * 2]"#,
        r#"[ <function>, <function> ] : List (Num a -> Num a)"#,
    )
}

#[test]
fn function_in_record() {
    expect_success(
        r#"{ n: 1, adder: \x -> x + 1 }"#,
        r#"{ adder: <function>, n: <function> } : { adder : Num a -> Num a, n : Num * }"#,
    )
}

#[test]
fn function_in_unwrapped_record() {
    expect_success(
        r#"{ adder: \x -> x + 1 }"#,
        r#"{ adder: <function> } : { adder : Num a -> Num a }"#,
    )
}

#[test]
fn function_in_tag() {
    expect_success(
        r#"Adder (\x -> x + 1)"#,
        r#"Adder <function> : [ Adder (Num a -> Num a) ]*"#,
    )
}

#[test]
fn newtype_of_record_of_tag_of_record_of_tag() {
    expect_success(
        r#"A {b: C {d: 1}}"#,
        r#"A { b: C { d: 1 } } : [ A { b : [ C { d : Num * } ]* } ]*"#,
    )
}

#[test]
fn print_u8s() {
    expect_success(
        indoc!(
            r#"
                x : U8
                x = 129
                x
                "#
        ),
        "129 : U8",
    )
}

#[test]
fn parse_problem() {
    expect_failure(
        "add m n = m + n",
        indoc!(
            r#"
                ── ARGUMENTS BEFORE EQUALS ─────────────────────────────────────────────────────

                I am partway through parsing a definition, but I got stuck here:

                1│  app "app" provides [ replOutput ] to "./platform"
                2│
                3│  replOutput =
                4│      add m n = m + n
                            ^^^

                Looks like you are trying to define a function. In roc, functions are
                always written as a lambda, like increment = \n -> n + 1.
                "#
        ),
    );
}

#[test]
fn mono_problem() {
    expect_failure(
        r#"
            t : [A, B, C]
            t = A

            when t is
                A -> "a"
            "#,
        indoc!(
            r#"
                ── UNSAFE PATTERN ──────────────────────────────────────────────────────────────

                This when does not cover all the possibilities:

                7│>                  when t is
                8│>                      A -> "a"

                Other possibilities include:

                    B
                    C

                I would have to crash if I saw one of those! Add branches for them!


                Enter an expression, or :help, or :exit/:q."#
        ),
    );
}

#[test]
fn issue_2343_complete_mono_with_shadowed_vars() {
    expect_failure(
        indoc!(
            r#"
                b = False
                f = \b ->
                    when b is
                        True -> 5
                        False -> 15
                f b
                "#
        ),
        indoc!(
            r#"
                ── DUPLICATE NAME ──────────────────────────────────────────────────────────────

                The b name is first defined here:

                4│      b = False
                        ^

                But then it's defined a second time here:

                5│      f = \b ->
                             ^

                Since these variables have the same name, it's easy to use the wrong
                one on accident. Give one of them a new name.
                "#
        ),
    );
}