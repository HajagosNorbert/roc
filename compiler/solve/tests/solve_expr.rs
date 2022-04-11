#[macro_use]
extern crate pretty_assertions;
#[macro_use]
extern crate indoc;

extern crate bumpalo;

mod helpers;

#[cfg(test)]
mod solve_expr {
    use crate::helpers::with_larger_debug_stack;
    use roc_types::pretty_print::{content_to_string, name_all_type_vars};

    // HELPERS

    fn infer_eq_help(
        src: &str,
    ) -> Result<
        (
            Vec<roc_solve::solve::TypeError>,
            Vec<roc_problem::can::Problem>,
            String,
        ),
        std::io::Error,
    > {
        use bumpalo::Bump;
        use std::fs::File;
        use std::io::Write;
        use std::path::PathBuf;
        use tempfile::tempdir;

        let arena = &Bump::new();

        let module_src;
        let temp;
        if src.starts_with("app") {
            // this is already a module
            module_src = src;
        } else {
            // this is an expression, promote it to a module
            temp = promote_expr_to_module(src);
            module_src = &temp;
        }

        let exposed_types = Default::default();
        let loaded = {
            let dir = tempdir()?;
            let filename = PathBuf::from("Test.roc");
            let file_path = dir.path().join(filename);
            let full_file_path = file_path.clone();
            let mut file = File::create(file_path)?;
            writeln!(file, "{}", module_src)?;
            drop(file);
            let result = roc_load::load_and_typecheck(
                arena,
                full_file_path,
                dir.path(),
                exposed_types,
                roc_target::TargetInfo::default_x86_64(),
            );

            dir.close()?;

            result
        };

        let loaded = loaded.expect("failed to load module");

        use roc_load::LoadedModule;
        let LoadedModule {
            module_id: home,
            mut can_problems,
            mut type_problems,
            interns,
            mut solved,
            exposed_to_host,
            ..
        } = loaded;

        let mut can_problems = can_problems.remove(&home).unwrap_or_default();
        let type_problems = type_problems.remove(&home).unwrap_or_default();

        let subs = solved.inner_mut();

        //        assert!(can_problems.is_empty());
        //        assert!(type_problems.is_empty());
        //        let CanExprOut {
        //            output,
        //            var_store,
        //            var,
        //            constraint,
        //            home,
        //            interns,
        //            problems: mut can_problems,
        //            ..
        //        } = can_expr(src);
        //        let mut subs = Subs::new(var_store.into());

        // TODO fix this
        // assert_correct_variable_usage(&constraint);

        // name type vars
        for var in exposed_to_host.values() {
            name_all_type_vars(*var, subs);
        }

        let content = {
            debug_assert!(exposed_to_host.len() == 1);
            let (_symbol, variable) = exposed_to_host.into_iter().next().unwrap();
            subs.get_content_without_compacting(variable)
        };

        let actual_str = content_to_string(content, subs, home, &interns);

        // Disregard UnusedDef problems, because those are unavoidable when
        // returning a function from the test expression.
        can_problems.retain(|prob| !matches!(prob, roc_problem::can::Problem::UnusedDef(_, _)));

        Ok((type_problems, can_problems, actual_str))
    }

    fn promote_expr_to_module(src: &str) -> String {
        let mut buffer =
            String::from("app \"test\" provides [ main ] to \"./platform\"\n\nmain =\n");

        for line in src.lines() {
            // indent the body!
            buffer.push_str("    ");
            buffer.push_str(line);
            buffer.push('\n');
        }

        buffer
    }

    fn infer_eq(src: &str, expected: &str) {
        let (_, can_problems, actual) = infer_eq_help(src).unwrap();

        assert_eq!(can_problems, Vec::new(), "Canonicalization problems: ");

        assert_eq!(actual, expected.to_string());
    }

    fn infer_eq_without_problem(src: &str, expected: &str) {
        let (type_problems, can_problems, actual) = infer_eq_help(src).unwrap();

        assert_eq!(can_problems, Vec::new(), "Canonicalization problems: ");

        if !type_problems.is_empty() {
            // fail with an assert, but print the problems normally so rust doesn't try to diff
            // an empty vec with the problems.
            panic!("expected:\n{:?}\ninferred:\n{:?}", expected, actual);
        }
        assert_eq!(actual, expected.to_string());
    }

    #[test]
    fn int_literal() {
        infer_eq("5", "Num *");
    }

    #[test]
    fn float_literal() {
        infer_eq("0.5", "Float *");
    }

    #[test]
    fn dec_literal() {
        infer_eq(
            indoc!(
                r#"
                    val : Dec
                    val = 1.2

                    val
                "#
            ),
            "Dec",
        );
    }

    #[test]
    fn string_literal() {
        infer_eq(
            indoc!(
                r#"
                    "type inference!"
                "#
            ),
            "Str",
        );
    }

    #[test]
    fn empty_string() {
        infer_eq(
            indoc!(
                r#"
                    ""
                "#
            ),
            "Str",
        );
    }

    #[test]
    fn string_starts_with() {
        infer_eq_without_problem(
            indoc!(
                r#"
                Str.startsWith
                "#
            ),
            "Str, Str -> Bool",
        );
    }

    #[test]
    fn string_from_int() {
        infer_eq_without_problem(
            indoc!(
                r#"
                Num.toStr
                "#
            ),
            "Num * -> Str",
        );
    }

    #[test]
    fn string_from_utf8() {
        infer_eq_without_problem(
            indoc!(
                r#"
                Str.fromUtf8
                "#
            ),
            "List U8 -> Result Str [ BadUtf8 Utf8ByteProblem Nat ]*",
        );
    }

    // #[test]
    // fn block_string_literal() {
    //     infer_eq(
    //         indoc!(
    //             r#"
    //             """type
    //             inference!"""
    //         "#
    //         ),
    //         "Str",
    //     );
    // }

    // LIST

    #[test]
    fn empty_list() {
        infer_eq(
            indoc!(
                r#"
                    []
                "#
            ),
            "List *",
        );
    }

    #[test]
    fn list_of_lists() {
        infer_eq(
            indoc!(
                r#"
                    [[]]
                "#
            ),
            "List (List *)",
        );
    }

    #[test]
    fn triple_nested_list() {
        infer_eq(
            indoc!(
                r#"
                    [[[]]]
                "#
            ),
            "List (List (List *))",
        );
    }

    #[test]
    fn nested_empty_list() {
        infer_eq(
            indoc!(
                r#"
                    [ [], [ [] ] ]
                "#
            ),
            "List (List (List *))",
        );
    }

    #[test]
    fn concat_different_types() {
        infer_eq(
            indoc!(
                r#"
                empty = []
                one = List.concat [ 1 ] empty
                str = List.concat [ "blah" ] empty

                empty
            "#
            ),
            "List *",
        );
    }

    #[test]
    fn list_of_one_int() {
        infer_eq(
            indoc!(
                r#"
                    [42]
                "#
            ),
            "List (Num *)",
        );
    }

    #[test]
    fn triple_nested_int_list() {
        infer_eq(
            indoc!(
                r#"
                    [[[ 5 ]]]
                "#
            ),
            "List (List (List (Num *)))",
        );
    }

    #[test]
    fn list_of_ints() {
        infer_eq(
            indoc!(
                r#"
                    [ 1, 2, 3 ]
                "#
            ),
            "List (Num *)",
        );
    }

    #[test]
    fn nested_list_of_ints() {
        infer_eq(
            indoc!(
                r#"
                    [ [ 1 ], [ 2, 3 ] ]
                "#
            ),
            "List (List (Num *))",
        );
    }

    #[test]
    fn list_of_one_string() {
        infer_eq(
            indoc!(
                r#"
                    [ "cowabunga" ]
                "#
            ),
            "List Str",
        );
    }

    #[test]
    fn triple_nested_string_list() {
        infer_eq(
            indoc!(
                r#"
                    [[[ "foo" ]]]
                "#
            ),
            "List (List (List Str))",
        );
    }

    #[test]
    fn list_of_strings() {
        infer_eq(
            indoc!(
                r#"
                    [ "foo", "bar" ]
                "#
            ),
            "List Str",
        );
    }

    // INTERPOLATED STRING

    #[test]
    fn infer_interpolated_string() {
        infer_eq(
            indoc!(
                r#"
                whatItIs = "great"

                "type inference is \(whatItIs)!"
            "#
            ),
            "Str",
        );
    }

    #[test]
    fn infer_interpolated_var() {
        infer_eq(
            indoc!(
                r#"
                whatItIs = "great"

                str = "type inference is \(whatItIs)!"

                whatItIs
            "#
            ),
            "Str",
        );
    }

    #[test]
    fn infer_interpolated_field() {
        infer_eq(
            indoc!(
                r#"
                rec = { whatItIs: "great" }

                str = "type inference is \(rec.whatItIs)!"

                rec
            "#
            ),
            "{ whatItIs : Str }",
        );
    }

    // LIST MISMATCH

    #[test]
    fn mismatch_heterogeneous_list() {
        infer_eq(
            indoc!(
                r#"
                    [ "foo", 5 ]
                "#
            ),
            "List <type mismatch>",
        );
    }

    #[test]
    fn mismatch_heterogeneous_nested_list() {
        infer_eq(
            indoc!(
                r#"
                    [ [ "foo", 5 ] ]
                "#
            ),
            "List (List <type mismatch>)",
        );
    }

    #[test]
    fn mismatch_heterogeneous_nested_empty_list() {
        infer_eq(
            indoc!(
                r#"
                [ [ 1 ], [ [] ] ]
            "#
            ),
            "List <type mismatch>",
        );
    }

    // CLOSURE

    #[test]
    fn always_return_empty_record() {
        infer_eq(
            indoc!(
                r#"
                    \_ -> {}
                "#
            ),
            "* -> {}",
        );
    }

    #[test]
    fn two_arg_return_int() {
        infer_eq(
            indoc!(
                r#"
                    \_, _ -> 42
                "#
            ),
            "*, * -> Num *",
        );
    }

    #[test]
    fn three_arg_return_string() {
        infer_eq(
            indoc!(
                r#"
                    \_, _, _ -> "test!"
                "#
            ),
            "*, *, * -> Str",
        );
    }

    // DEF

    #[test]
    fn def_empty_record() {
        infer_eq(
            indoc!(
                r#"
                    foo = {}

                    foo
                "#
            ),
            "{}",
        );
    }

    #[test]
    fn def_string() {
        infer_eq(
            indoc!(
                r#"
                    str = "thing"

                    str
                "#
            ),
            "Str",
        );
    }

    #[test]
    fn def_1_arg_closure() {
        infer_eq(
            indoc!(
                r#"
                    fn = \_ -> {}

                    fn
                "#
            ),
            "* -> {}",
        );
    }

    #[test]
    fn applied_tag() {
        infer_eq_without_problem(
            indoc!(
                r#"
                List.map [ "a", "b" ] \elem -> Foo elem
                "#
            ),
            "List [ Foo Str ]*",
        )
    }

    // Tests (TagUnion, Func)
    #[test]
    fn applied_tag_function() {
        infer_eq_without_problem(
            indoc!(
                r#"
                foo = Foo

                foo "hi"
                "#
            ),
            "[ Foo Str ]*",
        )
    }

    // Tests (TagUnion, Func)
    #[test]
    fn applied_tag_function_list_map() {
        infer_eq_without_problem(
            indoc!(
                r#"
                List.map [ "a", "b" ] Foo
                "#
            ),
            "List [ Foo Str ]*",
        )
    }

    // Tests (TagUnion, Func)
    #[test]
    fn applied_tag_function_list() {
        infer_eq_without_problem(
            indoc!(
                r#"
                [ \x -> Bar x, Foo ]
                "#
            ),
            "List (a -> [ Bar a, Foo a ]*)",
        )
    }

    // Tests (Func, TagUnion)
    #[test]
    fn applied_tag_function_list_other_way() {
        infer_eq_without_problem(
            indoc!(
                r#"
                [ Foo, \x -> Bar x ]
                "#
            ),
            "List (a -> [ Bar a, Foo a ]*)",
        )
    }

    // Tests (Func, TagUnion)
    #[test]
    fn applied_tag_function_record() {
        infer_eq_without_problem(
            indoc!(
                r#"
                foo = Foo

                {
                    x: [ foo, Foo ],
                    y: [ foo, \x -> Foo x ],
                    z: [ foo, \x,y  -> Foo x y ]
                }
                "#
            ),
            "{ x : List [ Foo ]*, y : List (a -> [ Foo a ]*), z : List (b, c -> [ Foo b c ]*) }",
        )
    }

    // Tests (TagUnion, Func)
    #[test]
    fn applied_tag_function_with_annotation() {
        infer_eq_without_problem(
            indoc!(
                r#"
                x : List [ Foo I64 ]
                x = List.map [ 1, 2 ] Foo

                x
                "#
            ),
            "List [ Foo I64 ]",
        )
    }

    #[test]
    fn def_2_arg_closure() {
        infer_eq(
            indoc!(
                r#"
                    func = \_, _ -> 42

                    func
                "#
            ),
            "*, * -> Num *",
        );
    }

    #[test]
    fn def_3_arg_closure() {
        infer_eq(
            indoc!(
                r#"
                    f = \_, _, _ -> "test!"

                    f
                "#
            ),
            "*, *, * -> Str",
        );
    }

    #[test]
    fn def_multiple_functions() {
        infer_eq(
            indoc!(
                r#"
                    a = \_, _, _ -> "test!"

                    b = a

                    b
                "#
            ),
            "*, *, * -> Str",
        );
    }

    #[test]
    fn def_multiple_strings() {
        infer_eq(
            indoc!(
                r#"
                    a = "test!"

                    b = a

                    b
                "#
            ),
            "Str",
        );
    }

    #[test]
    fn def_multiple_ints() {
        infer_eq(
            indoc!(
                r#"
                    c = b

                    b = a

                    a = 42

                    c
                "#
            ),
            "Num *",
        );
    }

    #[test]
    fn def_returning_closure() {
        infer_eq(
            indoc!(
                r#"
                    f = \z -> z
                    g = \z -> z

                    (\x ->
                        a = f x
                        b = g x
                        x
                    )
                "#
            ),
            "a -> a",
        );
    }

    // CALLING FUNCTIONS

    #[test]
    fn call_returns_int() {
        infer_eq(
            indoc!(
                r#"
                    alwaysFive = \_ -> 5

                    alwaysFive "stuff"
                "#
            ),
            "Num *",
        );
    }

    #[test]
    fn identity_returns_given_type() {
        infer_eq(
            indoc!(
                r#"
                    identity = \a -> a

                    identity "hi"
                "#
            ),
            "Str",
        );
    }

    #[test]
    fn identity_infers_principal_type() {
        infer_eq(
            indoc!(
                r#"
                    identity = \x -> x

                    y = identity 5

                    identity
                "#
            ),
            "a -> a",
        );
    }

    #[test]
    fn identity_works_on_incompatible_types() {
        infer_eq(
            indoc!(
                r#"
                    identity = \a -> a

                    x = identity 5
                    y = identity "hi"

                    x
                "#
            ),
            "Num *",
        );
    }

    #[test]
    fn call_returns_list() {
        infer_eq(
            indoc!(
                r#"
                    enlist = \val -> [ val ]

                    enlist 5
                "#
            ),
            "List (Num *)",
        );
    }

    #[test]
    fn indirect_always() {
        infer_eq(
            indoc!(
                r#"
                    always = \val -> (\_ -> val)
                    alwaysFoo = always "foo"

                    alwaysFoo 42
                "#
            ),
            "Str",
        );
    }

    #[test]
    fn pizza_desugar() {
        infer_eq(
            indoc!(
                r#"
                    1 |> (\a -> a)
                "#
            ),
            "Num *",
        );
    }

    #[test]
    fn pizza_desugar_two_arguments() {
        infer_eq(
            indoc!(
                r#"
                always2 = \a, _ -> a

                1 |> always2 "foo"
                "#
            ),
            "Num *",
        );
    }

    #[test]
    fn anonymous_identity() {
        infer_eq(
            indoc!(
                r#"
                    (\a -> a) 3.14
                "#
            ),
            "Float *",
        );
    }

    #[test]
    fn identity_of_identity() {
        infer_eq(
            indoc!(
                r#"
                    (\val -> val) (\val -> val)
                "#
            ),
            "a -> a",
        );
    }

    #[test]
    fn recursive_identity() {
        infer_eq(
            indoc!(
                r#"
                    identity = \val -> val

                    identity identity
                "#
            ),
            "a -> a",
        );
    }

    #[test]
    fn identity_function() {
        infer_eq(
            indoc!(
                r#"
                    \val -> val
                "#
            ),
            "a -> a",
        );
    }

    #[test]
    fn use_apply() {
        infer_eq(
            indoc!(
                r#"
                identity = \a -> a
                apply = \f, x -> f x

                apply identity 5
                "#
            ),
            "Num *",
        );
    }

    #[test]
    fn apply_function() {
        infer_eq(
            indoc!(
                r#"
                    \f, x -> f x
                "#
            ),
            "(a -> b), a -> b",
        );
    }

    // #[test]
    // TODO FIXME this should pass, but instead fails to canonicalize
    // fn use_flip() {
    //     infer_eq(
    //         indoc!(
    //             r#"
    //                 flip = \f -> (\a b -> f b a)
    //                 neverendingInt = \f int -> f int
    //                 x = neverendingInt (\a -> a) 5

    //                 flip neverendingInt
    //             "#
    //         ),
    //         "(Num *, (a -> a)) -> Num *",
    //     );
    // }

    #[test]
    fn flip_function() {
        infer_eq(
            indoc!(
                r#"
                    \f -> (\a, b -> f b a)
                "#
            ),
            "(a, b -> c) -> (b, a -> c)",
        );
    }

    #[test]
    fn always_function() {
        infer_eq(
            indoc!(
                r#"
                    \val -> \_ -> val
                "#
            ),
            "a -> (* -> a)",
        );
    }

    #[test]
    fn pass_a_function() {
        infer_eq(
            indoc!(
                r#"
                    \f -> f {}
                "#
            ),
            "({} -> a) -> a",
        );
    }

    // OPERATORS

    // #[test]
    // fn div_operator() {
    //     infer_eq(
    //         indoc!(
    //             r#"
    //             \l r -> l / r
    //         "#
    //         ),
    //         "F64, F64 -> F64",
    //     );
    // }

    //     #[test]
    //     fn basic_float_division() {
    //         infer_eq(
    //             indoc!(
    //                 r#"
    //                 1 / 2
    //             "#
    //             ),
    //             "F64",
    //         );
    //     }

    //     #[test]
    //     fn basic_int_division() {
    //         infer_eq(
    //             indoc!(
    //                 r#"
    //                 1 // 2
    //             "#
    //             ),
    //             "Num *",
    //         );
    //     }

    //     #[test]
    //     fn basic_addition() {
    //         infer_eq(
    //             indoc!(
    //                 r#"
    //                 1 + 2
    //             "#
    //             ),
    //             "Num *",
    //         );
    //     }

    // #[test]
    // fn basic_circular_type() {
    //     infer_eq(
    //         indoc!(
    //             r#"
    //             \x -> x x
    //         "#
    //         ),
    //         "<Type Mismatch: Circular Type>",
    //     );
    // }

    // #[test]
    // fn y_combinator_has_circular_type() {
    //     assert_eq!(
    //         infer(indoc!(r#"
    //             \f -> (\x -> f x x) (\x -> f x x)
    //         "#)),
    //         Erroneous(Problem::CircularType)
    //     );
    // }

    // #[test]
    // fn no_higher_ranked_types() {
    //     // This should error because it can't type of alwaysFive
    //     infer_eq(
    //         indoc!(
    //             r#"
    //             \always -> [ always [], always "" ]
    //        "#
    //         ),
    //         "<type mismatch>",
    //     );
    // }

    #[test]
    fn always_with_list() {
        infer_eq(
            indoc!(
                r#"
                    alwaysFive = \_ -> 5

                    [ alwaysFive "foo", alwaysFive [] ]
                "#
            ),
            "List (Num *)",
        );
    }

    #[test]
    fn if_with_int_literals() {
        infer_eq(
            indoc!(
                r#"
                    if True then
                        42
                    else
                        24
                "#
            ),
            "Num *",
        );
    }

    #[test]
    fn when_with_int_literals() {
        infer_eq(
            indoc!(
                r#"
                    when 1 is
                    1 -> 2
                    3 -> 4
                "#
            ),
            "Num *",
        );
    }

    // RECORDS

    #[test]
    fn empty_record() {
        infer_eq("{}", "{}");
    }

    #[test]
    fn one_field_record() {
        infer_eq("{ x: 5 }", "{ x : Num * }");
    }

    #[test]
    fn two_field_record() {
        infer_eq("{ x: 5, y : 3.14 }", "{ x : Num *, y : Float * }");
    }

    #[test]
    fn record_literal_accessor() {
        infer_eq("{ x: 5, y : 3.14 }.x", "Num *");
    }

    #[test]
    fn record_arg() {
        infer_eq("\\rec -> rec.x", "{ x : a }* -> a");
    }

    #[test]
    fn record_with_bound_var() {
        infer_eq(
            indoc!(
                r#"
                    fn = \rec ->
                        x = rec.x

                        rec

                    fn
                "#
            ),
            "{ x : a }b -> { x : a }b",
        );
    }

    #[test]
    fn using_type_signature() {
        infer_eq(
            indoc!(
                r#"
                    bar : custom -> custom
                    bar = \x -> x

                    bar
                "#
            ),
            "custom -> custom",
        );
    }

    #[test]
    fn type_signature_without_body() {
        infer_eq(
            indoc!(
                r#"
                    foo: Str -> {}

                    foo "hi"
                "#
            ),
            "{}",
        );
    }

    #[test]
    fn type_signature_without_body_rigid() {
        infer_eq(
            indoc!(
                r#"
                    foo : Num * -> custom

                    foo 2
                "#
            ),
            "custom",
        );
    }

    #[test]
    fn accessor_function() {
        infer_eq(".foo", "{ foo : a }* -> a");
    }

    #[test]
    fn type_signature_without_body_record() {
        infer_eq(
            indoc!(
                r#"
                    { x, y } : { x : ({} -> custom), y : {} }

                    x
                "#
            ),
            "{} -> custom",
        );
    }

    #[test]
    fn empty_record_pattern() {
        infer_eq(
            indoc!(
                r#"
                    # technically, an empty record can be destructured
                    {} = {}
                    thunk = \{} -> 42

                    xEmpty = if thunk {} == 42 then { x: {} } else { x: {} }

                    when xEmpty is
                        { x: {} } -> {}
                "#
            ),
            "{}",
        );
    }

    #[test]
    fn record_type_annotation() {
        // check that a closed record remains closed
        infer_eq(
            indoc!(
                r#"
                foo : { x : custom } -> custom
                foo = \{ x } -> x

                foo
            "#
            ),
            "{ x : custom } -> custom",
        );
    }

    #[test]
    fn record_update() {
        infer_eq(
            indoc!(
                r#"
                    user = { year: "foo", name: "Sam" }

                    { user & year: "foo" }
                "#
            ),
            "{ name : Str, year : Str }",
        );
    }

    #[test]
    fn bare_tag() {
        infer_eq(
            indoc!(
                r#"
                    Foo
                "#
            ),
            "[ Foo ]*",
        );
    }

    #[test]
    fn single_tag_pattern() {
        infer_eq(
            indoc!(
                r#"
                    \Foo -> 42
                "#
            ),
            "[ Foo ] -> Num *",
        );
    }

    #[test]
    fn single_private_tag_pattern() {
        infer_eq(
            indoc!(
                r#"
                    \@Foo -> 42
                "#
            ),
            "[ @Foo ] -> Num *",
        );
    }

    #[test]
    fn two_tag_pattern() {
        infer_eq(
            indoc!(
                r#"
                    \x ->
                        when x is
                            True -> 1
                            False -> 0
                "#
            ),
            "[ False, True ] -> Num *",
        );
    }

    #[test]
    fn tag_application() {
        infer_eq(
            indoc!(
                r#"
                    Foo "happy" 2020
                "#
            ),
            "[ Foo Str (Num *) ]*",
        );
    }

    #[test]
    fn private_tag_application() {
        infer_eq(
            indoc!(
                r#"
                    @Foo "happy" 2020
                "#
            ),
            "[ @Foo Str (Num *) ]*",
        );
    }

    #[test]
    fn record_extraction() {
        infer_eq(
            indoc!(
                r#"
                    f = \x ->
                        when x is
                            { a, b: _ } -> a

                    f
                "#
            ),
            "{ a : a, b : * }* -> a",
        );
    }

    #[test]
    fn record_field_pattern_match_with_guard() {
        infer_eq(
            indoc!(
                r#"
                    when { x: 5 } is
                        { x: 4 } -> 4
                "#
            ),
            "Num *",
        );
    }

    #[test]
    fn tag_union_pattern_match() {
        infer_eq(
            indoc!(
                r#"
                    \Foo x -> Foo x
                "#
            ),
            "[ Foo a ] -> [ Foo a ]*",
        );
    }

    #[test]
    fn tag_union_pattern_match_ignored_field() {
        infer_eq(
            indoc!(
                r#"
                    \Foo x _ -> Foo x "y"
                "#
            ),
            "[ Foo a * ] -> [ Foo a Str ]*",
        );
    }

    #[test]
    fn global_tag_with_field() {
        infer_eq(
            indoc!(
                r#"
                    when Foo "blah" is
                        Foo x -> x
                "#
            ),
            "Str",
        );
    }

    #[test]
    fn private_tag_with_field() {
        infer_eq(
            indoc!(
                r#"
                    when @Foo "blah" is
                        @Foo x -> x
                "#
            ),
            "Str",
        );
    }

    #[test]
    fn qualified_annotation_num_integer() {
        infer_eq(
            indoc!(
                r#"
                   int : Num.Num (Num.Integer Num.Signed64)

                   int
                "#
            ),
            "I64",
        );
    }
    #[test]
    fn qualified_annotated_num_integer() {
        infer_eq(
            indoc!(
                r#"
                   int : Num.Num (Num.Integer Num.Signed64)
                   int = 5

                   int
                "#
            ),
            "I64",
        );
    }
    #[test]
    fn annotation_num_integer() {
        infer_eq(
            indoc!(
                r#"
                   int : Num (Integer Signed64)

                   int
                "#
            ),
            "I64",
        );
    }
    #[test]
    fn annotated_num_integer() {
        infer_eq(
            indoc!(
                r#"
                   int : Num (Integer Signed64)
                   int = 5

                   int
                "#
            ),
            "I64",
        );
    }

    #[test]
    fn qualified_annotation_using_i128() {
        infer_eq(
            indoc!(
                r#"
                    int : Num.I128

                    int
                "#
            ),
            "I128",
        );
    }
    #[test]
    fn qualified_annotated_using_i128() {
        infer_eq(
            indoc!(
                r#"
                    int : Num.I128
                    int = 5

                    int
                "#
            ),
            "I128",
        );
    }
    #[test]
    fn annotation_using_i128() {
        infer_eq(
            indoc!(
                r#"
                    int : I128

                    int
                "#
            ),
            "I128",
        );
    }
    #[test]
    fn annotated_using_i128() {
        infer_eq(
            indoc!(
                r#"
                    int : I128
                    int = 5

                    int
                "#
            ),
            "I128",
        );
    }

    #[test]
    fn qualified_annotation_using_u128() {
        infer_eq(
            indoc!(
                r#"
                    int : Num.U128

                    int
                "#
            ),
            "U128",
        );
    }
    #[test]
    fn qualified_annotated_using_u128() {
        infer_eq(
            indoc!(
                r#"
                    int : Num.U128
                    int = 5

                    int
                "#
            ),
            "U128",
        );
    }
    #[test]
    fn annotation_using_u128() {
        infer_eq(
            indoc!(
                r#"
                    int : U128

                    int
                "#
            ),
            "U128",
        );
    }
    #[test]
    fn annotated_using_u128() {
        infer_eq(
            indoc!(
                r#"
                    int : U128
                    int = 5

                    int
                "#
            ),
            "U128",
        );
    }

    #[test]
    fn qualified_annotation_using_i64() {
        infer_eq(
            indoc!(
                r#"
                    int : Num.I64

                    int
                "#
            ),
            "I64",
        );
    }
    #[test]
    fn qualified_annotated_using_i64() {
        infer_eq(
            indoc!(
                r#"
                    int : Num.I64
                    int = 5

                    int
                "#
            ),
            "I64",
        );
    }
    #[test]
    fn annotation_using_i64() {
        infer_eq(
            indoc!(
                r#"
                    int : I64

                    int
                "#
            ),
            "I64",
        );
    }
    #[test]
    fn annotated_using_i64() {
        infer_eq(
            indoc!(
                r#"
                    int : I64
                    int = 5

                    int
                "#
            ),
            "I64",
        );
    }

    #[test]
    fn qualified_annotation_using_u64() {
        infer_eq(
            indoc!(
                r#"
                    int : Num.U64

                    int
                "#
            ),
            "U64",
        );
    }
    #[test]
    fn qualified_annotated_using_u64() {
        infer_eq(
            indoc!(
                r#"
                    int : Num.U64
                    int = 5

                    int
                "#
            ),
            "U64",
        );
    }
    #[test]
    fn annotation_using_u64() {
        infer_eq(
            indoc!(
                r#"
                    int : U64

                    int
                "#
            ),
            "U64",
        );
    }
    #[test]
    fn annotated_using_u64() {
        infer_eq(
            indoc!(
                r#"
                    int : U64
                    int = 5

                    int
                "#
            ),
            "U64",
        );
    }

    #[test]
    fn qualified_annotation_using_i32() {
        infer_eq(
            indoc!(
                r#"
                    int : Num.I32

                    int
                "#
            ),
            "I32",
        );
    }
    #[test]
    fn qualified_annotated_using_i32() {
        infer_eq(
            indoc!(
                r#"
                    int : Num.I32
                    int = 5

                    int
                "#
            ),
            "I32",
        );
    }
    #[test]
    fn annotation_using_i32() {
        infer_eq(
            indoc!(
                r#"
                    int : I32

                    int
                "#
            ),
            "I32",
        );
    }
    #[test]
    fn annotated_using_i32() {
        infer_eq(
            indoc!(
                r#"
                    int : I32
                    int = 5

                    int
                "#
            ),
            "I32",
        );
    }

    #[test]
    fn qualified_annotation_using_u32() {
        infer_eq(
            indoc!(
                r#"
                    int : Num.U32

                    int
                "#
            ),
            "U32",
        );
    }
    #[test]
    fn qualified_annotated_using_u32() {
        infer_eq(
            indoc!(
                r#"
                    int : Num.U32
                    int = 5

                    int
                "#
            ),
            "U32",
        );
    }
    #[test]
    fn annotation_using_u32() {
        infer_eq(
            indoc!(
                r#"
                    int : U32

                    int
                "#
            ),
            "U32",
        );
    }
    #[test]
    fn annotated_using_u32() {
        infer_eq(
            indoc!(
                r#"
                    int : U32
                    int = 5

                    int
                "#
            ),
            "U32",
        );
    }

    #[test]
    fn qualified_annotation_using_i16() {
        infer_eq(
            indoc!(
                r#"
                    int : Num.I16

                    int
                "#
            ),
            "I16",
        );
    }
    #[test]
    fn qualified_annotated_using_i16() {
        infer_eq(
            indoc!(
                r#"
                    int : Num.I16
                    int = 5

                    int
                "#
            ),
            "I16",
        );
    }
    #[test]
    fn annotation_using_i16() {
        infer_eq(
            indoc!(
                r#"
                    int : I16

                    int
                "#
            ),
            "I16",
        );
    }
    #[test]
    fn annotated_using_i16() {
        infer_eq(
            indoc!(
                r#"
                    int : I16
                    int = 5

                    int
                "#
            ),
            "I16",
        );
    }

    #[test]
    fn qualified_annotation_using_u16() {
        infer_eq(
            indoc!(
                r#"
                    int : Num.U16

                    int
                "#
            ),
            "U16",
        );
    }
    #[test]
    fn qualified_annotated_using_u16() {
        infer_eq(
            indoc!(
                r#"
                    int : Num.U16
                    int = 5

                    int
                "#
            ),
            "U16",
        );
    }
    #[test]
    fn annotation_using_u16() {
        infer_eq(
            indoc!(
                r#"
                    int : U16

                    int
                "#
            ),
            "U16",
        );
    }
    #[test]
    fn annotated_using_u16() {
        infer_eq(
            indoc!(
                r#"
                    int : U16
                    int = 5

                    int
                "#
            ),
            "U16",
        );
    }

    #[test]
    fn qualified_annotation_using_i8() {
        infer_eq(
            indoc!(
                r#"
                    int : Num.I8

                    int
                "#
            ),
            "I8",
        );
    }
    #[test]
    fn qualified_annotated_using_i8() {
        infer_eq(
            indoc!(
                r#"
                    int : Num.I8
                    int = 5

                    int
                "#
            ),
            "I8",
        );
    }
    #[test]
    fn annotation_using_i8() {
        infer_eq(
            indoc!(
                r#"
                    int : I8

                    int
                "#
            ),
            "I8",
        );
    }
    #[test]
    fn annotated_using_i8() {
        infer_eq(
            indoc!(
                r#"
                    int : I8
                    int = 5

                    int
                "#
            ),
            "I8",
        );
    }

    #[test]
    fn qualified_annotation_using_u8() {
        infer_eq(
            indoc!(
                r#"
                    int : Num.U8

                    int
                "#
            ),
            "U8",
        );
    }
    #[test]
    fn qualified_annotated_using_u8() {
        infer_eq(
            indoc!(
                r#"
                    int : Num.U8
                    int = 5

                    int
                "#
            ),
            "U8",
        );
    }
    #[test]
    fn annotation_using_u8() {
        infer_eq(
            indoc!(
                r#"
                    int : U8

                    int
                "#
            ),
            "U8",
        );
    }
    #[test]
    fn annotated_using_u8() {
        infer_eq(
            indoc!(
                r#"
                    int : U8
                    int = 5

                    int
                "#
            ),
            "U8",
        );
    }

    #[test]
    fn qualified_annotation_num_floatingpoint() {
        infer_eq(
            indoc!(
                r#"
                   float : Num.Num (Num.FloatingPoint Num.Binary64)

                   float
                "#
            ),
            "F64",
        );
    }
    #[test]
    fn qualified_annotated_num_floatingpoint() {
        infer_eq(
            indoc!(
                r#"
                   float : Num.Num (Num.FloatingPoint Num.Binary64)
                   float = 5.5

                   float
                "#
            ),
            "F64",
        );
    }
    #[test]
    fn annotation_num_floatingpoint() {
        infer_eq(
            indoc!(
                r#"
                   float : Num (FloatingPoint Binary64)

                   float
                "#
            ),
            "F64",
        );
    }
    #[test]
    fn annotated_num_floatingpoint() {
        infer_eq(
            indoc!(
                r#"
                   float : Num (FloatingPoint Binary64)
                   float = 5.5

                   float
                "#
            ),
            "F64",
        );
    }

    #[test]
    fn qualified_annotation_f64() {
        infer_eq(
            indoc!(
                r#"
                   float : Num.F64

                   float
                "#
            ),
            "F64",
        );
    }
    #[test]
    fn qualified_annotated_f64() {
        infer_eq(
            indoc!(
                r#"
                   float : Num.F64
                   float = 5.5

                   float
                "#
            ),
            "F64",
        );
    }
    #[test]
    fn annotation_f64() {
        infer_eq(
            indoc!(
                r#"
                   float : F64

                   float
                "#
            ),
            "F64",
        );
    }
    #[test]
    fn annotated_f64() {
        infer_eq(
            indoc!(
                r#"
                   float : F64
                   float = 5.5

                   float
                "#
            ),
            "F64",
        );
    }

    #[test]
    fn qualified_annotation_f32() {
        infer_eq(
            indoc!(
                r#"
                   float : Num.F32

                   float
                "#
            ),
            "F32",
        );
    }
    #[test]
    fn qualified_annotated_f32() {
        infer_eq(
            indoc!(
                r#"
                   float : Num.F32
                   float = 5.5

                   float
                "#
            ),
            "F32",
        );
    }
    #[test]
    fn annotation_f32() {
        infer_eq(
            indoc!(
                r#"
                   float : F32

                   float
                "#
            ),
            "F32",
        );
    }
    #[test]
    fn annotated_f32() {
        infer_eq(
            indoc!(
                r#"
                   float : F32
                   float = 5.5

                   float
                "#
            ),
            "F32",
        );
    }

    #[test]
    fn fake_result_ok() {
        infer_eq(
            indoc!(
                r#"
                    Res a e : [ Okay a, Error e ]

                    ok : Res I64 *
                    ok = Okay 5

                    ok
                "#
            ),
            "Res I64 *",
        );
    }

    #[test]
    fn fake_result_err() {
        infer_eq(
            indoc!(
                r#"
                    Res a e : [ Okay a, Error e ]

                    err : Res * Str
                    err = Error "blah"

                    err
                "#
            ),
            "Res * Str",
        );
    }

    #[test]
    fn basic_result_ok() {
        infer_eq(
            indoc!(
                r#"
                    ok : Result I64 *
                    ok = Ok 5

                    ok
                "#
            ),
            "Result I64 *",
        );
    }

    #[test]
    fn basic_result_err() {
        infer_eq(
            indoc!(
                r#"
                    err : Result * Str
                    err = Err "blah"

                    err
                "#
            ),
            "Result * Str",
        );
    }

    #[test]
    fn basic_result_conditional() {
        infer_eq(
            indoc!(
                r#"
                    ok : Result I64 *
                    ok = Ok 5

                    err : Result * Str
                    err = Err "blah"

                    if 1 > 0 then
                        ok
                    else
                        err
                "#
            ),
            "Result I64 Str",
        );
    }

    // #[test]
    // fn annotation_using_num_used() {
    //     // There was a problem where `I64`, because it is only an annotation
    //     // wasn't added to the vars_by_symbol.
    //     infer_eq_without_problem(
    //         indoc!(
    //             r#"
    //                int : I64

    //                p = (\x -> x) int

    //                p
    //                "#
    //         ),
    //         "I64",
    //     );
    // }

    #[test]
    fn num_identity() {
        infer_eq_without_problem(
            indoc!(
                r#"
                    numIdentity : Num.Num a -> Num.Num a
                    numIdentity = \x -> x

                    y = numIdentity 3.14

                    { numIdentity, x : numIdentity 42, y }
                "#
            ),
            "{ numIdentity : Num a -> Num a, x : Num a, y : F64 }",
        );
    }

    #[test]
    fn when_with_annotation() {
        infer_eq_without_problem(
            indoc!(
                r#"
                    x : Num.Num (Num.Integer Num.Signed64)
                    x =
                        when 2 is
                            3 -> 4
                            _ -> 5

                    x
                "#
            ),
            "I64",
        );
    }

    // TODO add more realistic function when able
    #[test]
    fn integer_sum() {
        infer_eq_without_problem(
            indoc!(
                r#"
                    f = \n ->
                        when n is
                            0 -> 0
                            _ -> f n

                    f
                "#
            ),
            "Num * -> Num *",
        );
    }

    #[test]
    fn identity_map() {
        infer_eq_without_problem(
            indoc!(
                r#"
                    map : (a -> b), [ Identity a ] -> [ Identity b ]
                    map = \f, identity ->
                        when identity is
                            Identity v -> Identity (f v)
                    map
                "#
            ),
            "(a -> b), [ Identity a ] -> [ Identity b ]",
        );
    }

    #[test]
    fn to_bit() {
        infer_eq_without_problem(
            indoc!(
                r#"
                   toBit = \bool ->
                       when bool is
                           True -> 1
                           False -> 0

                   toBit
                "#
            ),
            "[ False, True ] -> Num *",
        );
    }

    // this test is related to a bug where ext_var would have an incorrect rank.
    // This match has duplicate cases, but that's not important because exhaustiveness happens
    // after inference.
    #[test]
    fn to_bit_record() {
        infer_eq_without_problem(
            indoc!(
                r#"
                    foo = \rec ->
                        when rec is
                            { x: _ } -> "1"
                            { y: _ } -> "2"

                    foo
                "#
            ),
            "{ x : *, y : * }* -> Str",
        );
    }

    #[test]
    fn from_bit() {
        infer_eq_without_problem(
            indoc!(
                r#"
                   fromBit = \int ->
                       when int is
                           0 -> False
                           _ -> True

                   fromBit
                "#
            ),
            "Num * -> [ False, True ]*",
        );
    }

    #[test]
    fn result_map_explicit() {
        infer_eq_without_problem(
            indoc!(
                r#"
                    map : (a -> b), [ Err e, Ok a ] -> [ Err e, Ok b ]
                    map = \f, result ->
                        when result is
                            Ok v -> Ok (f v)
                            Err e -> Err e

                    map
                "#
            ),
            "(a -> b), [ Err e, Ok a ] -> [ Err e, Ok b ]",
        );
    }

    #[test]
    fn result_map_alias() {
        infer_eq_without_problem(
            indoc!(
                r#"
                    Res e a : [ Ok a, Err e ]

                    map : (a -> b), Res e a -> Res e b
                    map = \f, result ->
                        when result is
                            Ok v -> Ok (f v)
                            Err e -> Err e

                    map
                       "#
            ),
            "(a -> b), Res e a -> Res e b",
        );
    }

    #[test]
    fn record_from_load() {
        infer_eq_without_problem(
            indoc!(
                r#"
                    foo = \{ x } -> x

                    foo { x: 5 }
                "#
            ),
            "Num *",
        );
    }

    #[test]
    fn defs_from_load() {
        infer_eq_without_problem(
            indoc!(
                r#"
                    alwaysThreePointZero = \_ -> 3.0

                    answer = 42

                    identity = \a -> a

                    threePointZero = identity (alwaysThreePointZero {})

                    threePointZero
                "#
            ),
            "Float *",
        );
    }

    #[test]
    fn use_as_in_signature() {
        infer_eq_without_problem(
            indoc!(
                r#"
                    foo : Str.Str as Foo -> Foo
                    foo = \_ -> "foo"

                    foo
                "#
            ),
            "Foo -> Foo",
        );
    }

    #[test]
    fn use_alias_in_let() {
        infer_eq_without_problem(
            indoc!(
                r#"
                    Foo : Str.Str

                    foo : Foo -> Foo
                    foo = \_ -> "foo"

                    foo
                "#
            ),
            "Foo -> Foo",
        );
    }

    #[test]
    fn use_alias_with_argument_in_let() {
        infer_eq_without_problem(
            indoc!(
                r#"
                Foo a : { foo : a }

                v : Foo (Num.Num (Num.Integer Num.Signed64))
                v = { foo: 42 }

                v
                "#
            ),
            "Foo I64",
        );
    }

    #[test]
    fn identity_alias() {
        infer_eq_without_problem(
            indoc!(
                r#"
                Foo a : { foo : a }

                id : Foo a -> Foo a
                id = \x -> x

                id
                "#
            ),
            "Foo a -> Foo a",
        );
    }

    #[test]
    fn linked_list_empty() {
        infer_eq_without_problem(
            indoc!(
                r#"
                    empty : [ Cons a (ConsList a), Nil ] as ConsList a
                    empty = Nil

                    empty
                       "#
            ),
            "ConsList a",
        );
    }

    #[test]
    fn linked_list_singleton() {
        infer_eq_without_problem(
            indoc!(
                r#"
                    singleton : a -> [ Cons a (ConsList a), Nil ] as ConsList a
                    singleton = \x -> Cons x Nil

                    singleton
                       "#
            ),
            "a -> ConsList a",
        );
    }

    #[test]
    fn peano_length() {
        infer_eq_without_problem(
            indoc!(
                r#"
                    Peano : [ S Peano, Z ]

                    length : Peano -> Num.Num (Num.Integer Num.Signed64)
                    length = \peano ->
                        when peano is
                            Z -> 0
                            S v -> length v

                    length
                       "#
            ),
            "Peano -> I64",
        );
    }

    #[test]
    fn peano_map() {
        infer_eq_without_problem(
            indoc!(
                r#"
                    map : [ S Peano, Z ] as Peano -> Peano
                    map = \peano ->
                        when peano is
                            Z -> Z
                            S v -> S (map v)

                    map
                       "#
            ),
            "Peano -> Peano",
        );
    }

    #[test]
    fn infer_linked_list_map() {
        infer_eq_without_problem(
            indoc!(
                r#"
                    map = \f, list ->
                        when list is
                            Nil -> Nil
                            Cons x xs ->
                                a = f x
                                b = map f xs

                                Cons a b

                    map
                       "#
            ),
            "(a -> b), [ Cons a c, Nil ] as c -> [ Cons b d, Nil ]* as d",
        );
    }

    #[test]
    fn typecheck_linked_list_map() {
        infer_eq_without_problem(
            indoc!(
                r#"
                    ConsList a : [ Cons a (ConsList a), Nil ]

                    map : (a -> b), ConsList a -> ConsList b
                    map = \f, list ->
                        when list is
                            Nil -> Nil
                            Cons x xs ->
                                Cons (f x) (map f xs)

                    map
                       "#
            ),
            "(a -> b), ConsList a -> ConsList b",
        );
    }

    #[test]
    fn mismatch_in_alias_args_gets_reported() {
        infer_eq(
            indoc!(
                r#"
                Foo a : a

                r : Foo {}
                r = {}

                s : Foo Str.Str
                s = "bar"

                when {} is
                    _ -> s
                    _ -> r
                "#
            ),
            "<type mismatch>",
        );
    }

    #[test]
    fn mismatch_in_apply_gets_reported() {
        infer_eq(
            indoc!(
                r#"
                r : { x : (Num.Num (Num.Integer Signed64)) }
                r = { x : 1 }

                s : { left : { x : Num.Num (Num.FloatingPoint Num.Binary64) } }
                s = { left: { x : 3.14 } }

                when 0 is
                    1 -> s.left
                    0 -> r
                   "#
            ),
            "<type mismatch>",
        );
    }

    #[test]
    fn mismatch_in_tag_gets_reported() {
        infer_eq(
            indoc!(
                r#"
                r : [ Ok Str.Str ]
                r = Ok 1

                s : { left: [ Ok {} ] }
                s = { left: Ok 3.14  }

                when 0 is
                    1 -> s.left
                    0 -> r
                   "#
            ),
            "<type mismatch>",
        );
    }

    // TODO As intended, this fails, but it fails with the wrong error!
    //
    // #[test]
    // fn nums() {
    //     infer_eq_without_problem(
    //         indoc!(
    //             r#"
    //                 s : Num *
    //                 s = 3.1

    //                 s
    //                 "#
    //         ),
    //         "<Type Mismatch: _____________>",
    //     );
    // }

    #[test]
    fn peano_map_alias() {
        infer_eq(
            indoc!(
                r#"
                app "test" provides [ main ] to "./platform"

                Peano : [ S Peano, Z ]

                map : Peano -> Peano
                map = \peano ->
                        when peano is
                            Z -> Z
                            S rest -> S (map rest)

                main =
                    map
                "#
            ),
            "Peano -> Peano",
        );
    }

    #[test]
    fn unit_alias() {
        infer_eq(
            indoc!(
                r#"
                    Unit : [ Unit ]

                    unit : Unit
                    unit = Unit

                    unit
                "#
            ),
            "Unit",
        );
    }

    #[test]
    fn rigid_in_letnonrec() {
        infer_eq_without_problem(
            indoc!(
                r#"
                    ConsList a : [ Cons a (ConsList a), Nil ]

                    toEmpty : ConsList a -> ConsList a
                    toEmpty = \_ ->
                        result : ConsList a
                        result = Nil

                        result

                    toEmpty
                "#
            ),
            "ConsList a -> ConsList a",
        );
    }

    #[test]
    fn rigid_in_letrec_ignored() {
        // re-enable when we don't capture local things that don't need to be!
        infer_eq_without_problem(
            indoc!(
                r#"
                    ConsList a : [ Cons a (ConsList a), Nil ]

                    toEmpty : ConsList a -> ConsList a
                    toEmpty = \_ ->
                        result : ConsList a
                        result = Nil

                        toEmpty result

                    toEmpty
                "#
            ),
            "ConsList a -> ConsList a",
        );
    }

    #[test]
    fn rigid_in_letrec() {
        infer_eq_without_problem(
            indoc!(
                r#"
                app "test" provides [ main ] to "./platform"

                ConsList a : [ Cons a (ConsList a), Nil ]

                toEmpty : ConsList a -> ConsList a
                toEmpty = \_ ->
                    result : ConsList a
                    result = Nil

                    toEmpty result

                main =
                    toEmpty
                "#
            ),
            "ConsList a -> ConsList a",
        );
    }

    #[test]
    fn let_record_pattern_with_annotation() {
        infer_eq_without_problem(
            indoc!(
                r#"
                    { x, y } : { x : Str.Str, y : Num.Num (Num.FloatingPoint Num.Binary64) }
                    { x, y } = { x : "foo", y : 3.14 }

                    x
                "#
            ),
            "Str",
        );
    }

    #[test]
    fn let_record_pattern_with_annotation_alias() {
        infer_eq(
            indoc!(
                r#"
                    Foo : { x : Str.Str, y : Num.Num (Num.FloatingPoint Num.Binary64) }

                    { x, y } : Foo
                    { x, y } = { x : "foo", y : 3.14 }

                    x
                "#
            ),
            "Str",
        );
    }

    #[test]
    fn peano_map_infer() {
        infer_eq(
            indoc!(
                r#"
                app "test" provides [ main ] to "./platform"

                map =
                    \peano ->
                        when peano is
                            Z -> Z
                            S rest -> map rest |> S


                main =
                    map
                "#
            ),
            "[ S a, Z ] as a -> [ S b, Z ]* as b",
        );
    }

    #[test]
    fn peano_map_infer_nested() {
        infer_eq(
            indoc!(
                r#"
                    map = \peano ->
                            when peano is
                                Z -> Z
                                S rest ->
                                    map rest |> S


                    map
                "#
            ),
            "[ S a, Z ] as a -> [ S b, Z ]* as b",
        );
    }

    #[test]
    fn let_record_pattern_with_alias_annotation() {
        infer_eq_without_problem(
            indoc!(
                r#"
                    Foo : { x : Str.Str, y : Num.Num (Num.FloatingPoint Num.Binary64) }

                    { x, y } : Foo
                    { x, y } = { x : "foo", y : 3.14 }

                    x
               "#
            ),
            "Str",
        );
    }

    #[test]
    fn let_tag_pattern_with_annotation() {
        infer_eq_without_problem(
            indoc!(
                r#"
                     UserId x : [ UserId I64 ]
                     UserId x = UserId 42

                     x
                 "#
            ),
            "I64",
        );
    }

    #[test]
    fn typecheck_record_linked_list_map() {
        infer_eq_without_problem(
            indoc!(
                r#"
                    ConsList q : [ Cons { x: q, xs: ConsList q }, Nil ]

                    map : (a -> b), ConsList a -> ConsList b
                    map = \f, list ->
                        when list is
                            Nil -> Nil
                            Cons { x,  xs } ->
                                Cons { x: f x, xs : map f xs }

                    map
                "#
            ),
            "(a -> b), ConsList a -> ConsList b",
        );
    }

    #[test]
    fn infer_record_linked_list_map() {
        infer_eq_without_problem(
            indoc!(
                r#"
                    map = \f, list ->
                        when list is
                            Nil -> Nil
                            Cons { x,  xs } ->
                                Cons { x: f x, xs : map f xs }

                    map
                "#
            ),
            "(a -> b), [ Cons { x : a, xs : c }*, Nil ] as c -> [ Cons { x : b, xs : d }, Nil ]* as d",
        );
    }

    #[test]
    fn typecheck_mutually_recursive_tag_union_2() {
        infer_eq_without_problem(
            indoc!(
                r#"
                    ListA a b : [ Cons a (ListB b a), Nil ]
                    ListB a b : [ Cons a (ListA b a), Nil ]

                    ConsList q : [ Cons q (ConsList q), Nil ]

                    toAs : (b -> a), ListA a b -> ConsList a
                    toAs = \f, lista ->
                        when lista is
                            Nil -> Nil
                            Cons a listb ->
                                when listb is
                                    Nil -> Nil
                                    Cons b newLista ->
                                        Cons a (Cons (f b) (toAs f newLista))

                    toAs
                "#
            ),
            "(b -> a), ListA a b -> ConsList a",
        );
    }

    #[test]
    fn typecheck_mutually_recursive_tag_union_listabc() {
        infer_eq_without_problem(
            indoc!(
                r#"
                    ListA a : [ Cons a (ListB a) ]
                    ListB a : [ Cons a (ListC a) ]
                    ListC a : [ Cons a (ListA a), Nil ]

                    val : ListC Num.I64
                    val = Cons 1 (Cons 2 (Cons 3 Nil))

                    val
                "#
            ),
            "ListC I64",
        );
    }

    #[test]
    fn infer_mutually_recursive_tag_union() {
        infer_eq_without_problem(
            indoc!(
                r#"
                   toAs = \f, lista ->
                        when lista is
                            Nil -> Nil
                            Cons a listb ->
                                when listb is
                                    Nil -> Nil
                                    Cons b newLista ->
                                        Cons a (Cons (f b) (toAs f newLista))

                   toAs
                "#
            ),
            "(a -> b), [ Cons c [ Cons a d, Nil ], Nil ] as d -> [ Cons c [ Cons b e ]*, Nil ]* as e"
        );
    }

    #[test]
    fn solve_list_get() {
        infer_eq_without_problem(
            indoc!(
                r#"
                    List.get [ "a" ] 0
                "#
            ),
            "Result Str [ OutOfBounds ]*",
        );
    }

    #[test]
    fn type_more_general_than_signature() {
        infer_eq_without_problem(
            indoc!(
                r#"
                partition : Nat, Nat, List (Int a) -> [ Pair Nat (List (Int a)) ]
                partition = \low, high, initialList ->
                    when List.get initialList high is
                        Ok _ ->
                            Pair 0 []

                        Err _ ->
                            Pair (low - 1) initialList

                partition
                            "#
            ),
            "Nat, Nat, List (Int a) -> [ Pair Nat (List (Int a)) ]",
        );
    }

    #[test]
    fn quicksort_partition() {
        with_larger_debug_stack(|| {
            infer_eq_without_problem(
                indoc!(
                    r#"
                swap : Nat, Nat, List a -> List a
                swap = \i, j, list ->
                    when Pair (List.get list i) (List.get list j) is
                        Pair (Ok atI) (Ok atJ) ->
                            list
                                |> List.set i atJ
                                |> List.set j atI

                        _ ->
                            list

                partition : Nat, Nat, List (Int a) -> [ Pair Nat (List (Int a)) ]
                partition = \low, high, initialList ->
                    when List.get initialList high is
                        Ok pivot ->
                            go = \i, j, list ->
                                if j < high then
                                    when List.get list j is
                                        Ok value ->
                                            if value <= pivot then
                                                go (i + 1) (j + 1) (swap (i + 1) j list)
                                            else
                                                go i (j + 1) list

                                        Err _ ->
                                            Pair i list
                                else
                                    Pair i list

                            when go (low - 1) low initialList is
                                Pair newI newList ->
                                    Pair (newI + 1) (swap (newI + 1) high newList)

                        Err _ ->
                            Pair (low - 1) initialList

                partition
            "#
                ),
                "Nat, Nat, List (Int a) -> [ Pair Nat (List (Int a)) ]",
            );
        });
    }

    #[test]
    fn identity_list() {
        infer_eq_without_problem(
            indoc!(
                r#"
                idList : List a -> List a
                idList = \list -> list

                foo : List I64 -> List I64
                foo = \initialList -> idList initialList


                foo
            "#
            ),
            "List I64 -> List I64",
        );
    }

    #[test]
    fn list_get() {
        infer_eq_without_problem(
            indoc!(
                r#"
                    List.get [ 10, 9, 8, 7 ] 1
                "#
            ),
            "Result (Num *) [ OutOfBounds ]*",
        );

        infer_eq_without_problem(
            indoc!(
                r#"
                    List.get
                "#
            ),
            "List a, Nat -> Result a [ OutOfBounds ]*",
        );
    }

    #[test]
    fn use_rigid_twice() {
        infer_eq_without_problem(
            indoc!(
                r#"
                id1 : q -> q
                id1 = \x -> x

                id2 : q -> q
                id2 = \x -> x

                { id1, id2 }
                "#
            ),
            "{ id1 : q -> q, id2 : q -> q }",
        );
    }

    #[test]
    fn map_insert() {
        infer_eq_without_problem(
            indoc!(
                r#"
                Dict.insert
                "#
            ),
            "Dict a b, a, b -> Dict a b",
        );
    }

    #[test]
    fn num_to_float() {
        infer_eq_without_problem(
            indoc!(
                r#"
                Num.toFloat
                "#
            ),
            "Num * -> Float *",
        );
    }

    #[test]
    fn pow() {
        infer_eq_without_problem(
            indoc!(
                r#"
                Num.pow
                "#
            ),
            "Float a, Float a -> Float a",
        );
    }

    #[test]
    fn ceiling() {
        infer_eq_without_problem(
            indoc!(
                r#"
                Num.ceiling
                "#
            ),
            "Float * -> Int *",
        );
    }

    #[test]
    fn floor() {
        infer_eq_without_problem(
            indoc!(
                r#"
                Num.floor
                "#
            ),
            "Float * -> Int *",
        );
    }

    #[test]
    fn div_ceil() {
        infer_eq_without_problem(
            indoc!(
                r#"
                Num.divCeil
                "#
            ),
            "Int a, Int a -> Result (Int a) [ DivByZero ]*",
        );
    }

    #[test]
    fn pow_int() {
        infer_eq_without_problem(
            indoc!(
                r#"
                Num.powInt
                "#
            ),
            "Int a, Int a -> Int a",
        );
    }

    #[test]
    fn atan() {
        infer_eq_without_problem(
            indoc!(
                r#"
                Num.atan
                "#
            ),
            "Float a -> Float a",
        );
    }

    #[test]
    fn min_i128() {
        infer_eq_without_problem(
            indoc!(
                r#"
                Num.minI128
                "#
            ),
            "I128",
        );
    }

    #[test]
    fn max_i128() {
        infer_eq_without_problem(
            indoc!(
                r#"
                Num.maxI128
                "#
            ),
            "I128",
        );
    }

    #[test]
    fn min_i64() {
        infer_eq_without_problem(
            indoc!(
                r#"
                Num.minI64
                "#
            ),
            "I64",
        );
    }

    #[test]
    fn max_i64() {
        infer_eq_without_problem(
            indoc!(
                r#"
                Num.maxI64
                "#
            ),
            "I64",
        );
    }

    #[test]
    fn min_u64() {
        infer_eq_without_problem(
            indoc!(
                r#"
                Num.minU64
                "#
            ),
            "U64",
        );
    }

    #[test]
    fn max_u64() {
        infer_eq_without_problem(
            indoc!(
                r#"
                Num.maxU64
                "#
            ),
            "U64",
        );
    }

    #[test]
    fn min_i32() {
        infer_eq_without_problem(
            indoc!(
                r#"
                Num.minI32
                "#
            ),
            "I32",
        );
    }

    #[test]
    fn max_i32() {
        infer_eq_without_problem(
            indoc!(
                r#"
                Num.maxI32
                "#
            ),
            "I32",
        );
    }

    #[test]
    fn min_u32() {
        infer_eq_without_problem(
            indoc!(
                r#"
                Num.minU32
                "#
            ),
            "U32",
        );
    }

    #[test]
    fn max_u32() {
        infer_eq_without_problem(
            indoc!(
                r#"
                Num.maxU32
                "#
            ),
            "U32",
        );
    }

    #[test]
    fn reconstruct_path() {
        infer_eq_without_problem(
            indoc!(
                r#"
                reconstructPath : Dict position position, position -> List position
                reconstructPath = \cameFrom, goal ->
                    when Dict.get cameFrom goal is
                        Err KeyNotFound ->
                            []

                        Ok next ->
                            List.append (reconstructPath cameFrom next) goal

                reconstructPath
                "#
            ),
            "Dict position position, position -> List position",
        );
    }

    #[test]
    fn use_correct_ext_record() {
        // Related to a bug solved in 81fbab0b3fe4765bc6948727e603fc2d49590b1c
        infer_eq_without_problem(
            indoc!(
                r#"
                f = \r ->
                    g = r.q
                    h = r.p

                    42

                f
                "#
            ),
            "{ p : *, q : * }* -> Num *",
        );
    }

    #[test]
    fn use_correct_ext_tag_union() {
        // related to a bug solved in 08c82bf151a85e62bce02beeed1e14444381069f
        infer_eq_without_problem(
            indoc!(
                r#"
                app "test" provides [ main ] to "./platform"

                boom = \_ -> boom {}

                Model position : { openSet : Set position }

                cheapestOpen : Model position -> Result position [ KeyNotFound ]*
                cheapestOpen = \model ->

                    folder = \resSmallestSoFar, position ->
                                    when resSmallestSoFar is
                                        Err _ -> resSmallestSoFar
                                        Ok smallestSoFar ->
                                            if position == smallestSoFar.position then resSmallestSoFar

                                            else
                                                Ok { position, cost: 0.0 }

                    Set.walk model.openSet (Ok { position: boom {}, cost: 0.0 }) folder
                        |> Result.map (\x -> x.position)

                astar : Model position -> Result position [ KeyNotFound ]*
                astar = \model -> cheapestOpen model

                main =
                    astar
                "#
            ),
            "Model position -> Result position [ KeyNotFound ]*",
        );
    }

    #[test]
    fn when_with_or_pattern_and_guard() {
        infer_eq_without_problem(
            indoc!(
                r#"
                \x ->
                    when x is
                        2 | 3 -> 0
                        a if a < 20 ->  1
                        3 | 4 if False -> 2
                        _ -> 3
                "#
            ),
            "Num * -> Num *",
        );
    }

    #[test]
    #[ignore]
    fn sorting() {
        // based on https://github.com/elm/compiler/issues/2057
        // Roc seems to do this correctly, tracking to make sure it stays that way
        infer_eq_without_problem(
            indoc!(
                r#"
                sort : ConsList cm -> ConsList cm
                sort =
                    \xs ->
                        f : cm, cm -> Order
                        f = \_, _ -> LT

                        sortWith f xs

                sortBy : (x -> cmpl), ConsList x -> ConsList x
                sortBy =
                    \_, list ->
                        cmp : x, x -> Order
                        cmp = \_, _ -> LT

                        sortWith cmp list

                always = \x, _ -> x

                sortWith : (foobar, foobar -> Order), ConsList foobar -> ConsList foobar
                sortWith =
                    \_, list ->
                        f = \arg ->
                            g arg

                        g = \bs ->
                            when bs is
                                bx -> f bx
                                _ -> Nil

                        always Nil (f list)

                Order : [ LT, GT, EQ ]
                ConsList a : [ Nil, Cons a (ConsList a) ]

                { x: sortWith, y: sort, z: sortBy }
                "#
            ),
            "{ x : (foobar, foobar -> Order), ConsList foobar -> ConsList foobar, y : ConsList cm -> ConsList cm, z : (x -> cmpl), ConsList x -> ConsList x }"
        );
    }

    // Like in elm, this test now fails. Polymorphic recursion (even with an explicit signature)
    // yields a type error.
    //
    // We should at some point investigate why that is. Elm did support polymorphic recursion in
    // earlier versions.
    //
    //    #[test]
    //    fn wrapper() {
    //        // based on https://github.com/elm/compiler/issues/1964
    //        // Roc seems to do this correctly, tracking to make sure it stays that way
    //        infer_eq_without_problem(
    //            indoc!(
    //                r#"
    //                Type a : [ TypeCtor (Type (Wrapper a)) ]
    //
    //                Wrapper a : [ Wrapper a ]
    //
    //                Opaque : [ Opaque ]
    //
    //                encodeType1 : Type a -> Opaque
    //                encodeType1 = \thing ->
    //                    when thing is
    //                        TypeCtor v0 ->
    //                            encodeType1 v0
    //
    //                encodeType1
    //                "#
    //            ),
    //            "Type a -> Opaque",
    //        );
    //    }

    #[test]
    fn rigids() {
        infer_eq_without_problem(
            indoc!(
                r#"
                f : List a -> List a
                f = \input ->
                    # let-polymorphism at work
                    x : List b
                    x = []

                    when List.get input 0 is
                        Ok val -> List.append x val
                        Err _ -> input
                f
                "#
            ),
            "List a -> List a",
        );
    }

    #[cfg(debug_assertions)]
    #[test]
    #[should_panic]
    fn rigid_record_quantification() {
        // the ext here is qualified on the outside (because we have rank 1 types, not rank 2).
        // That means e.g. `f : { bar : String, foo : I64 } -> Bool }` is a valid argument, but
        // that function could not be applied to the `{ foo : I64 }` list. Therefore, this function
        // is not allowed.
        //
        // should hit a debug_assert! in debug mode, and produce a type error in release mode
        infer_eq_without_problem(
            indoc!(
                r#"
                test : ({ foo : I64 }ext -> Bool), { foo : I64 } -> Bool
                test = \fn, a -> fn a

                test
                "#
            ),
            "should fail",
        );
    }

    // OPTIONAL RECORD FIELDS

    #[test]
    fn optional_field_unifies_with_missing() {
        infer_eq_without_problem(
            indoc!(
                r#"
                    negatePoint : { x : I64, y : I64, z ? Num c } -> { x : I64, y : I64, z : Num c }

                    negatePoint { x: 1, y: 2 }
                "#
            ),
            "{ x : I64, y : I64, z : Num c }",
        );
    }

    #[test]
    fn open_optional_field_unifies_with_missing() {
        infer_eq_without_problem(
            indoc!(
                r#"
                    negatePoint : { x : I64, y : I64, z ? Num c }r -> { x : I64, y : I64, z : Num c }r

                    a = negatePoint { x: 1, y: 2 }
                    b = negatePoint { x: 1, y: 2, blah : "hi" }

                    { a, b }
                "#
            ),
            "{ a : { x : I64, y : I64, z : Num c }, b : { blah : Str, x : I64, y : I64, z : Num c } }",
        );
    }

    #[test]
    fn optional_field_unifies_with_present() {
        infer_eq_without_problem(
            indoc!(
                r#"
                    negatePoint : { x : Num a, y : Num b, z ? c } -> { x : Num a, y : Num b, z : c }

                    negatePoint { x: 1, y: 2.1, z: 0x3 }
                "#
            ),
            "{ x : Num a, y : F64, z : Int * }",
        );
    }

    #[test]
    fn open_optional_field_unifies_with_present() {
        infer_eq_without_problem(
            indoc!(
                r#"
                    negatePoint : { x : Num a, y : Num b, z ? c }r -> { x : Num a, y : Num b, z : c }r

                    a = negatePoint { x: 1, y: 2.1 }
                    b = negatePoint { x: 1, y: 2.1, blah : "hi" }

                    { a, b }
                "#
            ),
            "{ a : { x : Num a, y : F64, z : c }, b : { blah : Str, x : Num a, y : F64, z : c } }",
        );
    }

    #[test]
    fn optional_field_function() {
        infer_eq_without_problem(
            indoc!(
                r#"
                \{ x, y ? 0 } -> x + y
                "#
            ),
            "{ x : Num a, y ? Num a }* -> Num a",
        );
    }

    #[test]
    fn optional_field_let() {
        infer_eq_without_problem(
            indoc!(
                r#"
                { x, y ? 0 } = { x: 32 }

                x + y
                "#
            ),
            "Num *",
        );
    }

    #[test]
    fn optional_field_when() {
        infer_eq_without_problem(
            indoc!(
                r#"
                \r ->
                    when r is
                        { x, y ? 0 } -> x + y
                "#
            ),
            "{ x : Num a, y ? Num a }* -> Num a",
        );
    }

    #[test]
    fn optional_field_let_with_signature() {
        infer_eq_without_problem(
            indoc!(
                r#"
                \rec ->
                    { x, y } : { x : I64, y ? Bool }*
                    { x, y ? False } = rec

                    { x, y }
                "#
            ),
            "{ x : I64, y ? Bool }* -> { x : I64, y : Bool }",
        );
    }

    #[test]
    fn list_walk_backwards() {
        infer_eq_without_problem(
            indoc!(
                r#"
                List.walkBackwards
                "#
            ),
            "List a, b, (b, a -> b) -> b",
        );
    }

    #[test]
    fn list_walk_backwards_example() {
        infer_eq_without_problem(
            indoc!(
                r#"
                empty : List I64
                empty =
                    []

                List.walkBackwards empty 0 (\a, b -> a + b)
                "#
            ),
            "I64",
        );
    }

    #[test]
    fn list_drop_at() {
        infer_eq_without_problem(
            indoc!(
                r#"
                List.dropAt
                "#
            ),
            "List a, Nat -> List a",
        );
    }

    #[test]
    fn str_trim() {
        infer_eq_without_problem(
            indoc!(
                r#"
                Str.trim
                "#
            ),
            "Str -> Str",
        );
    }

    #[test]
    fn str_trim_left() {
        infer_eq_without_problem(
            indoc!(
                r#"
                Str.trimLeft
                "#
            ),
            "Str -> Str",
        );
    }

    #[test]
    fn list_take_first() {
        infer_eq_without_problem(
            indoc!(
                r#"
                List.takeFirst
                "#
            ),
            "List a, Nat -> List a",
        );
    }

    #[test]
    fn list_take_last() {
        infer_eq_without_problem(
            indoc!(
                r#"
                List.takeLast
                "#
            ),
            "List a, Nat -> List a",
        );
    }

    #[test]
    fn list_sublist() {
        infer_eq_without_problem(
            indoc!(
                r#"
                List.sublist
                "#
            ),
            "List a, { len : Nat, start : Nat } -> List a",
        );
    }

    #[test]
    fn list_split() {
        infer_eq_without_problem(
            indoc!("List.split"),
            "List a, Nat -> { before : List a, others : List a }",
        );
    }

    #[test]
    fn list_drop_last() {
        infer_eq_without_problem(
            indoc!(
                r#"
                List.dropLast
                "#
            ),
            "List a -> List a",
        );
    }

    #[test]
    fn list_intersperse() {
        infer_eq_without_problem(
            indoc!(
                r#"
                List.intersperse
                "#
            ),
            "List a, a -> List a",
        );
    }
    #[test]
    fn function_that_captures_nothing_is_not_captured() {
        // we should make sure that a function that doesn't capture anything it not itself captured
        // such functions will be lifted to the top-level, and are thus globally available!
        infer_eq_without_problem(
            indoc!(
                r#"
                f = \x -> x + 1

                g = \y -> f y

                g
                "#
            ),
            "Num a -> Num a",
        );
    }

    #[test]
    fn double_named_rigids() {
        infer_eq_without_problem(
            indoc!(
                r#"
                app "test" provides [ main ] to "./platform"


                main : List x
                main =
                    empty : List x
                    empty = []

                    empty
                "#
            ),
            "List x",
        );
    }

    #[test]
    fn double_tag_application() {
        infer_eq_without_problem(
            indoc!(
                r#"
                app "test" provides [ main ] to "./platform"


                main =
                    if 1 == 1 then
                        Foo (Bar) 1
                    else
                        Foo Bar 1
                "#
            ),
            "[ Foo [ Bar ]* (Num *) ]*",
        );

        infer_eq_without_problem("Foo Bar 1", "[ Foo [ Bar ]* (Num *) ]*");
    }

    #[test]
    fn double_tag_application_pattern_global() {
        infer_eq_without_problem(
            indoc!(
                r#"
                app "test" provides [ main ] to "./platform"

                Bar : [ Bar ]
                Foo : [ Foo Bar I64, Empty ]

                foo : Foo
                foo = Foo Bar 1

                main =
                    when foo is
                        Foo Bar 1 ->
                            Foo Bar 2

                        x ->
                            x
                "#
            ),
            "[ Empty, Foo Bar I64 ]",
        );
    }

    #[test]
    fn double_tag_application_pattern_private() {
        infer_eq_without_problem(
            indoc!(
                r#"
                app "test" provides [ main ] to "./platform"

                Foo : [ @Foo [ @Bar ] I64, @Empty ]

                foo : Foo
                foo = @Foo @Bar 1

                main =
                    when foo is
                        @Foo @Bar 1 ->
                            @Foo @Bar 2

                        x ->
                            x
                "#
            ),
            "[ @Empty, @Foo [ @Bar ] I64 ]",
        );
    }

    #[test]
    fn recursive_function_with_rigid() {
        infer_eq_without_problem(
            indoc!(
                r#"
                app "test" provides [ main ] to "./platform"

                State a : { count : I64, x : a }

                foo : State a -> I64
                foo = \state ->
                    if state.count == 0 then
                        0
                    else
                        1 + foo { count: state.count - 1, x: state.x }

                main : I64
                main =
                    foo { count: 3, x: {} }
                "#
            ),
            "I64",
        );
    }

    #[test]
    fn rbtree_empty() {
        infer_eq_without_problem(
            indoc!(
                r#"
                app "test" provides [ main ] to "./platform"

                # The color of a node. Leaves are considered Black.
                NodeColor : [ Red, Black ]

                RBTree k v : [ Node NodeColor k v (RBTree k v) (RBTree k v), Empty ]

                # Create an empty dictionary.
                empty : RBTree k v
                empty =
                    Empty

                foo : RBTree I64 I64
                foo = empty

                main : RBTree I64 I64
                main =
                    foo
                "#
            ),
            "RBTree I64 I64",
        );
    }

    #[test]
    fn rbtree_insert() {
        // exposed an issue where pattern variables were not introduced
        // at the correct level in the constraint
        //
        // see 22592eff805511fbe1da63849771ee5f367a6a16
        infer_eq_without_problem(
            indoc!(
                r#"
                app "test" provides [ main ] to "./platform"

                RBTree k : [ Node k (RBTree k), Empty ]

                balance : RBTree  k -> RBTree k
                balance = \left ->
                    when left is
                      Node _ Empty -> Empty

                      _ -> Empty

                main : RBTree {}
                main =
                    balance Empty
                "#
            ),
            "RBTree {}",
        );
    }

    #[test]
    #[ignore]
    fn rbtree_full_remove_min() {
        infer_eq_without_problem(
            indoc!(
                r#"
                app "test" provides [ main ] to "./platform"

                NodeColor : [ Red, Black ]

                RBTree k v : [ Node NodeColor k v (RBTree k v) (RBTree k v), Empty ]

                moveRedLeft : RBTree k v -> RBTree k v
                moveRedLeft = \dict ->
                  when dict is
                    # Node clr k v (Node lClr lK lV lLeft lRight) (Node rClr rK rV ((Node Red rlK rlV rlL rlR) as rLeft) rRight) ->
                    # Node clr k v (Node lClr lK lV lLeft lRight) (Node rClr rK rV rLeft rRight) ->
                    Node clr k v (Node _ lK lV lLeft lRight) (Node _ rK rV rLeft rRight) ->
                        when rLeft is
                            Node Red rlK rlV rlL rlR ->
                              Node
                                Red
                                rlK
                                rlV
                                (Node Black k v (Node Red lK lV lLeft lRight) rlL)
                                (Node Black rK rV rlR rRight)

                            _ ->
                              when clr is
                                Black ->
                                  Node
                                    Black
                                    k
                                    v
                                    (Node Red lK lV lLeft lRight)
                                    (Node Red rK rV rLeft rRight)

                                Red ->
                                  Node
                                    Black
                                    k
                                    v
                                    (Node Red lK lV lLeft lRight)
                                    (Node Red rK rV rLeft rRight)

                    _ ->
                      dict

                balance : NodeColor, k, v, RBTree k v, RBTree k v -> RBTree k v
                balance = \color, key, value, left, right ->
                  when right is
                    Node Red rK rV rLeft rRight ->
                      when left is
                        Node Red lK lV lLeft lRight ->
                          Node
                            Red
                            key
                            value
                            (Node Black lK lV lLeft lRight)
                            (Node Black rK rV rLeft rRight)

                        _ ->
                          Node color rK rV (Node Red key value left rLeft) rRight

                    _ ->
                      when left is
                        Node Red lK lV (Node Red llK llV llLeft llRight) lRight ->
                          Node
                            Red
                            lK
                            lV
                            (Node Black llK llV llLeft llRight)
                            (Node Black key value lRight right)

                        _ ->
                          Node color key value left right


                Key k : Num k

                removeHelpEQGT : Key k, RBTree (Key k) v -> RBTree (Key k) v
                removeHelpEQGT = \targetKey, dict ->
                  when dict is
                    Node color key value left right ->
                      if targetKey == key then
                        when getMin right is
                          Node _ minKey minValue _ _ ->
                            balance color minKey minValue left (removeMin right)

                          Empty ->
                            Empty
                      else
                        balance color key value left (removeHelp targetKey right)

                    Empty ->
                      Empty

                getMin : RBTree k v -> RBTree k v
                getMin = \dict ->
                  when dict is
                    # Node _ _ _ ((Node _ _ _ _ _) as left) _ ->
                    Node _ _ _ left _ ->
                        when left is
                            Node _ _ _ _ _ -> getMin left
                            _ -> dict

                    _ ->
                      dict


                moveRedRight : RBTree k v -> RBTree k v
                moveRedRight = \dict ->
                  when dict is
                    Node clr k v (Node lClr lK lV (Node Red llK llV llLeft llRight) lRight) (Node rClr rK rV rLeft rRight) ->
                      Node
                        Red
                        lK
                        lV
                        (Node Black llK llV llLeft llRight)
                        (Node Black k v lRight (Node Red rK rV rLeft rRight))

                    Node clr k v (Node lClr lK lV lLeft lRight) (Node rClr rK rV rLeft rRight) ->
                      when clr is
                        Black ->
                          Node
                            Black
                            k
                            v
                            (Node Red lK lV lLeft lRight)
                            (Node Red rK rV rLeft rRight)

                        Red ->
                          Node
                            Black
                            k
                            v
                            (Node Red lK lV lLeft lRight)
                            (Node Red rK rV rLeft rRight)

                    _ ->
                      dict


                removeHelpPrepEQGT : Key k, RBTree (Key k) v, NodeColor, (Key k), v, RBTree (Key k) v, RBTree (Key k) v -> RBTree (Key k) v
                removeHelpPrepEQGT = \_, dict, color, key, value, left, right ->
                  when left is
                    Node Red lK lV lLeft lRight ->
                      Node
                        color
                        lK
                        lV
                        lLeft
                        (Node Red key value lRight right)

                    _ ->
                      when right is
                        Node Black _ _ (Node Black _ _ _ _) _ ->
                          moveRedRight dict

                        Node Black _ _ Empty _ ->
                          moveRedRight dict

                        _ ->
                          dict


                removeMin : RBTree k v -> RBTree k v
                removeMin = \dict ->
                  when dict is
                    Node color key value left right ->
                        when left is
                            Node lColor _ _ lLeft _ ->
                              when lColor is
                                Black ->
                                  when lLeft is
                                    Node Red _ _ _ _ ->
                                      Node color key value (removeMin left) right

                                    _ ->
                                      when moveRedLeft dict is # here 1
                                        Node nColor nKey nValue nLeft nRight ->
                                          balance nColor nKey nValue (removeMin nLeft) nRight

                                        Empty ->
                                          Empty

                                _ ->
                                  Node color key value (removeMin left) right

                            _ ->
                                Empty
                    _ ->
                      Empty

                removeHelp : Key k, RBTree (Key k) v -> RBTree (Key k) v
                removeHelp = \targetKey, dict ->
                  when dict is
                    Empty ->
                      Empty

                    Node color key value left right ->
                      if targetKey < key then
                        when left is
                          Node Black _ _ lLeft _ ->
                            when lLeft is
                              Node Red _ _ _ _ ->
                                Node color key value (removeHelp targetKey left) right

                              _ ->
                                when moveRedLeft dict is # here 2
                                  Node nColor nKey nValue nLeft nRight ->
                                    balance nColor nKey nValue (removeHelp targetKey nLeft) nRight

                                  Empty ->
                                    Empty

                          _ ->
                            Node color key value (removeHelp targetKey left) right
                      else
                        removeHelpEQGT targetKey (removeHelpPrepEQGT targetKey dict color key value left right)


                main : RBTree I64 I64
                main =
                    removeHelp 1 Empty
                "#
            ),
            "RBTree I64 I64",
        );
    }

    #[test]
    fn rbtree_remove_min_1() {
        infer_eq_without_problem(
            indoc!(
                r#"
                app "test" provides [ main ] to "./platform"

                RBTree k : [ Node k (RBTree k) (RBTree k), Empty ]

                removeHelp : Num k, RBTree (Num k) -> RBTree (Num k)
                removeHelp = \targetKey, dict ->
                  when dict is
                    Empty ->
                      Empty

                    Node key left right ->
                      if targetKey < key then
                        when left is
                          Node _ lLeft _ ->
                            when lLeft is
                              Node _ _ _ ->
                                Empty

                              _ -> Empty

                          _ ->
                            Node key (removeHelp targetKey left) right
                      else
                        Empty


                main : RBTree I64
                main =
                    removeHelp 1 Empty
                "#
            ),
            "RBTree I64",
        );
    }

    #[test]
    fn rbtree_foobar() {
        infer_eq_without_problem(
            indoc!(
                r#"
                app "test" provides [ main ] to "./platform"

                NodeColor : [ Red, Black ]

                RBTree k v : [ Node NodeColor k v (RBTree k v) (RBTree k v), Empty ]

                removeHelp : Num k, RBTree (Num k) v -> RBTree (Num k) v
                removeHelp = \targetKey, dict ->
                  when dict is
                    Empty ->
                      Empty

                    Node color key value left right ->
                      if targetKey < key then
                        when left is
                          Node Black _ _ lLeft _ ->
                            when lLeft is
                              Node Red _ _ _ _ ->
                                Node color key value (removeHelp targetKey left) right

                              _ ->
                                when moveRedLeft dict is # here 2
                                  Node nColor nKey nValue nLeft nRight ->
                                    balance nColor nKey nValue (removeHelp targetKey nLeft) nRight

                                  Empty ->
                                    Empty

                          _ ->
                            Node color key value (removeHelp targetKey left) right
                      else
                        removeHelpEQGT targetKey (removeHelpPrepEQGT targetKey dict color key value left right)

                Key k : Num k

                balance : NodeColor, k, v, RBTree k v, RBTree k v -> RBTree k v

                moveRedLeft : RBTree k v -> RBTree k v

                removeHelpPrepEQGT : Key k, RBTree (Key k) v, NodeColor, (Key k), v, RBTree (Key k) v, RBTree (Key k) v -> RBTree (Key k) v

                removeHelpEQGT : Key k, RBTree (Key k) v -> RBTree (Key k) v
                removeHelpEQGT = \targetKey, dict ->
                  when dict is
                    Node color key value left right ->
                      if targetKey == key then
                        when getMin right is
                          Node _ minKey minValue _ _ ->
                            balance color minKey minValue left (removeMin right)

                          Empty ->
                            Empty
                      else
                        balance color key value left (removeHelp targetKey right)

                    Empty ->
                      Empty

                getMin : RBTree k v -> RBTree k v

                removeMin : RBTree k v -> RBTree k v

                main : RBTree I64 I64
                main =
                    removeHelp 1 Empty
                "#
            ),
            "RBTree I64 I64",
        );
    }

    #[test]
    fn quicksort_partition_help() {
        infer_eq_without_problem(
            indoc!(
                r#"
                app "test" provides [ partitionHelp ] to "./platform"

                swap : Nat, Nat, List a -> List a
                swap = \i, j, list ->
                    when Pair (List.get list i) (List.get list j) is
                        Pair (Ok atI) (Ok atJ) ->
                            list
                                |> List.set i atJ
                                |> List.set j atI

                        _ ->
                            []

                partitionHelp : Nat, Nat, List (Num a), Nat, (Num a) -> [ Pair Nat (List (Num a)) ]
                partitionHelp = \i, j, list, high, pivot ->
                    if j < high then
                        when List.get list j is
                            Ok value ->
                                if value <= pivot then
                                    partitionHelp (i + 1) (j + 1) (swap (i + 1) j list) high pivot
                                else
                                    partitionHelp i (j + 1) list high pivot

                            Err _ ->
                                Pair i list
                    else
                        Pair i list
                "#
            ),
            "Nat, Nat, List (Num a), Nat, Num a -> [ Pair Nat (List (Num a)) ]",
        );
    }

    #[test]
    fn rbtree_old_balance_simplified() {
        infer_eq_without_problem(
            indoc!(
                r#"
                app "test" provides [ main ] to "./platform"

                RBTree k : [ Node k (RBTree k) (RBTree k), Empty ]

                balance : k, RBTree k -> RBTree k
                balance = \key, left ->
                    Node key left Empty

                main : RBTree I64
                main =
                    balance 0 Empty
                "#
            ),
            "RBTree I64",
        );
    }

    #[test]
    fn rbtree_balance_simplified() {
        infer_eq_without_problem(
            indoc!(
                r#"
                app "test" provides [ main ] to "./platform"

                RBTree k : [ Node k (RBTree k) (RBTree k), Empty ]

                node = \x,y,z -> Node x y z

                balance : k, RBTree k -> RBTree k
                balance = \key, left ->
                    node key left Empty

                main : RBTree I64
                main =
                    balance 0 Empty
                "#
            ),
            "RBTree I64",
        );
    }

    #[test]
    fn rbtree_balance() {
        infer_eq_without_problem(
            indoc!(
                r#"
                app "test" provides [ main ] to "./platform"

                NodeColor : [ Red, Black ]

                RBTree k v : [ Node NodeColor k v (RBTree k v) (RBTree k v), Empty ]

                balance : NodeColor, k, v, RBTree k v, RBTree k v -> RBTree k v
                balance = \color, key, value, left, right ->
                  when right is
                    Node Red rK rV rLeft rRight ->
                      when left is
                        Node Red lK lV lLeft lRight ->
                          Node
                            Red
                            key
                            value
                            (Node Black lK lV lLeft lRight)
                            (Node Black rK rV rLeft rRight)

                        _ ->
                          Node color rK rV (Node Red key value left rLeft) rRight

                    _ ->
                      when left is
                        Node Red lK lV (Node Red llK llV llLeft llRight) lRight ->
                          Node
                            Red
                            lK
                            lV
                            (Node Black llK llV llLeft llRight)
                            (Node Black key value lRight right)

                        _ ->
                          Node color key value left right

                main : RBTree I64 I64
                main =
                    balance Red 0 0 Empty Empty
                "#
            ),
            "RBTree I64 I64",
        );
    }

    #[test]
    fn pattern_rigid_problem() {
        infer_eq_without_problem(
            indoc!(
                r#"
                app "test" provides [ main ] to "./platform"

                RBTree k : [ Node k (RBTree k) (RBTree k), Empty ]

                balance : k, RBTree k -> RBTree k
                balance = \key, left ->
                      when left is
                        Node _ _ lRight ->
                            Node key lRight Empty

                        _ ->
                            Empty


                main : RBTree I64
                main =
                    balance 0 Empty
                "#
            ),
            "RBTree I64",
        );
    }

    #[test]
    fn expr_to_str() {
        infer_eq_without_problem(
            indoc!(
                r#"
                app "test" provides [ main ] to "./platform"

                Expr : [ Add Expr Expr, Val I64, Var I64 ]

                printExpr : Expr -> Str
                printExpr = \e ->
                    when e is
                        Add a b ->
                            "Add ("
                                |> Str.concat (printExpr a)
                                |> Str.concat ") ("
                                |> Str.concat (printExpr b)
                                |> Str.concat ")"
                        Val v -> Num.toStr v
                        Var v -> "Var " |> Str.concat (Num.toStr v)

                main : Str
                main = printExpr (Var 3)
                "#
            ),
            "Str",
        );
    }

    #[test]
    fn int_type_let_polymorphism() {
        infer_eq_without_problem(
            indoc!(
                r#"
                app "test" provides [ main ] to "./platform"

                x = 4

                f : U8 -> U32
                f = \z -> Num.intCast z

                y = f x

                main =
                    x
                "#
            ),
            "Num *",
        );
    }

    #[test]
    fn rigid_type_variable_problem() {
        // see https://github.com/rtfeldman/roc/issues/1162
        infer_eq_without_problem(
            indoc!(
                r#"
        app "test" provides [ main ] to "./platform"

        RBTree k : [ Node k (RBTree k) (RBTree k), Empty ]

        balance : a, RBTree a -> RBTree a
        balance = \key, left ->
              when left is
                Node _ _ lRight ->
                    Node key lRight Empty

                _ ->
                    Empty


        main : RBTree {}
        main =
            balance {} Empty
            "#
            ),
            "RBTree {}",
        );
    }

    #[test]
    fn inference_var_inside_arrow() {
        infer_eq_without_problem(
            indoc!(
                r#"
                id : _ -> _
                id = \x -> x
                id
                "#
            ),
            "a -> a",
        )
    }

    #[test]
    fn inference_var_inside_ctor() {
        infer_eq_without_problem(
            indoc!(
                r#"
                canIGo : _ -> Result _ _
                canIGo = \color ->
                    when color is
                        "green" -> Ok "go!"
                        "yellow" -> Err (SlowIt "whoa, let's slow down!")
                        "red" -> Err (StopIt "absolutely not")
                        _ -> Err (UnknownColor "this is a weird stoplight")
                canIGo
                "#
            ),
            "Str -> Result Str [ SlowIt Str, StopIt Str, UnknownColor Str ]*",
        )
    }

    #[test]
    fn inference_var_inside_ctor_linked() {
        infer_eq_without_problem(
            indoc!(
                r#"
                swapRcd: {x: _, y: _} -> {x: _, y: _}
                swapRcd = \{x, y} -> {x: y, y: x}
                swapRcd
                "#
            ),
            "{ x : a, y : b } -> { x : b, y : a }",
        )
    }

    #[test]
    fn inference_var_link_with_rigid() {
        infer_eq_without_problem(
            indoc!(
                r#"
                swapRcd: {x: tx, y: ty} -> {x: _, y: _}
                swapRcd = \{x, y} -> {x: y, y: x}
                swapRcd
                "#
            ),
            "{ x : tx, y : ty } -> { x : ty, y : tx }",
        )
    }

    #[test]
    fn inference_var_inside_tag_ctor() {
        infer_eq_without_problem(
            indoc!(
                r#"
                badComics: Bool -> [ CowTools _, Thagomizer _ ]
                badComics = \c ->
                    when c is
                        True -> CowTools "The Far Side"
                        False ->  Thagomizer "The Far Side"
                badComics
                "#
            ),
            "Bool -> [ CowTools Str, Thagomizer Str ]",
        )
    }

    #[test]
    fn inference_var_tag_union_ext() {
        // TODO: we should really be inferring [ Blue, Orange ]a -> [ Lavender, Peach ]a here.
        // See https://github.com/rtfeldman/roc/issues/2053
        infer_eq_without_problem(
            indoc!(
                r#"
                pastelize: _ -> [ Lavender, Peach ]_
                pastelize = \color ->
                    when color is
                        Blue -> Lavender
                        Orange -> Peach
                        col -> col
                pastelize
                "#
            ),
            "[ Blue, Lavender, Orange, Peach ]a -> [ Blue, Lavender, Orange, Peach ]a",
        )
    }

    #[test]
    fn inference_var_rcd_union_ext() {
        infer_eq_without_problem(
            indoc!(
                r#"
                setRocEmail : _ -> { name: Str, email: Str }_
                setRocEmail = \person ->
                    { person & email: "\(person.name)@roclang.com" }
                setRocEmail
                "#
            ),
            "{ email : Str, name : Str }a -> { email : Str, name : Str }a",
        )
    }

    #[test]
    fn issue_2217() {
        infer_eq_without_problem(
            indoc!(
                r#"
                LinkedList elem : [Empty, Prepend (LinkedList elem) elem]

                fromList : List elem -> LinkedList elem
                fromList = \elems -> List.walk elems Empty Prepend

                fromList
                "#
            ),
            "List elem -> LinkedList elem",
        )
    }

    #[test]
    fn issue_2217_inlined() {
        infer_eq_without_problem(
            indoc!(
                r#"
                fromList : List elem -> [ Empty, Prepend (LinkedList elem) elem ] as LinkedList elem
                fromList = \elems -> List.walk elems Empty Prepend

                fromList
                "#
            ),
            "List elem -> LinkedList elem",
        )
    }

    #[test]
    fn infer_union_input_position1() {
        infer_eq_without_problem(
            indoc!(
                r#"
                 \tag ->
                     when tag is
                       A -> X
                       B -> Y
                 "#
            ),
            "[ A, B ] -> [ X, Y ]*",
        )
    }

    #[test]
    fn infer_union_input_position2() {
        infer_eq_without_problem(
            indoc!(
                r#"
                 \tag ->
                     when tag is
                       A -> X
                       B -> Y
                       _ -> Z
                 "#
            ),
            "[ A, B ]* -> [ X, Y, Z ]*",
        )
    }

    #[test]
    fn infer_union_input_position3() {
        infer_eq_without_problem(
            indoc!(
                r#"
                 \tag ->
                     when tag is
                       A M -> X
                       A N -> Y
                 "#
            ),
            "[ A [ M, N ] ] -> [ X, Y ]*",
        )
    }

    #[test]
    fn infer_union_input_position4() {
        infer_eq_without_problem(
            indoc!(
                r#"
                 \tag ->
                     when tag is
                       A M -> X
                       A N -> Y
                       A _ -> Z
                 "#
            ),
            "[ A [ M, N ]* ] -> [ X, Y, Z ]*",
        )
    }

    #[test]
    fn infer_union_input_position5() {
        infer_eq_without_problem(
            indoc!(
                r#"
                 \tag ->
                     when tag is
                       A (M J) -> X
                       A (N K) -> X
                 "#
            ),
            "[ A [ M [ J ], N [ K ] ] ] -> [ X ]*",
        )
    }

    #[test]
    fn infer_union_input_position6() {
        infer_eq_without_problem(
            indoc!(
                r#"
                 \tag ->
                     when tag is
                       A M -> X
                       B   -> X
                       A N -> X
                 "#
            ),
            "[ A [ M, N ], B ] -> [ X ]*",
        )
    }

    #[test]
    fn infer_union_input_position7() {
        infer_eq_without_problem(
            indoc!(
                r#"
                 \tag ->
                     when tag is
                         A -> X
                         t -> t
                 "#
            ),
            // TODO: we could be a bit smarter by subtracting "A" as a possible
            // tag in the union known by t, which would yield the principal type
            // [ A, ]a -> [ X ]a
            "[ A, X ]a -> [ A, X ]a",
        )
    }

    #[test]
    fn infer_union_input_position8() {
        infer_eq_without_problem(
            indoc!(
                r#"
                 \opt ->
                     when opt is
                         Some ({tag: A}) -> 1
                         Some ({tag: B}) -> 1
                         None -> 0
                 "#
            ),
            "[ None, Some { tag : [ A, B ] }* ] -> Num *",
        )
    }

    #[test]
    fn infer_union_input_position9() {
        infer_eq_without_problem(
            indoc!(
                r#"
                 opt : [ Some Str, None ]
                 opt = Some ""
                 rcd = { opt }

                 when rcd is
                     { opt: Some s } -> s
                     { opt: None } -> "?"
                 "#
            ),
            "Str",
        )
    }

    #[test]
    fn infer_union_input_position10() {
        infer_eq_without_problem(
            indoc!(
                r#"
                 \r ->
                     when r is
                         { x: Blue, y ? 3 } -> y
                         { x: Red, y ? 5 } -> y
                 "#
            ),
            "{ x : [ Blue, Red ], y ? Num a }* -> Num a",
        )
    }

    #[test]
    // Issue #2299
    fn infer_union_argument_position() {
        infer_eq_without_problem(
            indoc!(
                r#"
                 \UserId id -> id + 1
                 "#
            ),
            "[ UserId (Num a) ] -> Num a",
        )
    }

    #[test]
    fn infer_union_def_position() {
        infer_eq_without_problem(
            indoc!(
                r#"
                 \email ->
                    Email str = email
                    Str.isEmpty str
                 "#
            ),
            "[ Email Str ] -> Bool",
        )
    }

    #[test]
    fn numeric_literal_suffixes() {
        infer_eq_without_problem(
            indoc!(
                r#"
                {
                    u8:   123u8,
                    u16:  123u16,
                    u32:  123u32,
                    u64:  123u64,
                    u128: 123u128,

                    i8:   123i8,
                    i16:  123i16,
                    i32:  123i32,
                    i64:  123i64,
                    i128: 123i128,

                    nat:  123nat,

                    bu8:   0b11u8,
                    bu16:  0b11u16,
                    bu32:  0b11u32,
                    bu64:  0b11u64,
                    bu128: 0b11u128,

                    bi8:   0b11i8,
                    bi16:  0b11i16,
                    bi32:  0b11i32,
                    bi64:  0b11i64,
                    bi128: 0b11i128,

                    bnat:  0b11nat,

                    dec:  123.0dec,
                    f32:  123.0f32,
                    f64:  123.0f64,

                    fdec: 123dec,
                    ff32: 123f32,
                    ff64: 123f64,
                }
                "#
            ),
            r#"{ bi128 : I128, bi16 : I16, bi32 : I32, bi64 : I64, bi8 : I8, bnat : Nat, bu128 : U128, bu16 : U16, bu32 : U32, bu64 : U64, bu8 : U8, dec : Dec, f32 : F32, f64 : F64, fdec : Dec, ff32 : F32, ff64 : F64, i128 : I128, i16 : I16, i32 : I32, i64 : I64, i8 : I8, nat : Nat, u128 : U128, u16 : U16, u32 : U32, u64 : U64, u8 : U8 }"#,
        )
    }

    #[test]
    fn numeric_literal_suffixes_in_pattern() {
        infer_eq_without_problem(
            indoc!(
                r#"
                {
                    u8:   (\n ->
                            when n is
                              123u8 -> n),
                    u16:  (\n ->
                            when n is
                              123u16 -> n),
                    u32:  (\n ->
                            when n is
                              123u32 -> n),
                    u64:  (\n ->
                            when n is
                              123u64 -> n),
                    u128: (\n ->
                            when n is
                              123u128 -> n),

                    i8:   (\n ->
                            when n is
                              123i8 -> n),
                    i16:  (\n ->
                            when n is
                              123i16 -> n),
                    i32:  (\n ->
                            when n is
                              123i32 -> n),
                    i64:  (\n ->
                            when n is
                              123i64 -> n),
                    i128: (\n ->
                            when n is
                              123i128 -> n),

                    nat:  (\n ->
                            when n is
                              123nat -> n),

                    bu8:   (\n ->
                            when n is
                              0b11u8 -> n),
                    bu16:  (\n ->
                            when n is
                              0b11u16 -> n),
                    bu32:  (\n ->
                            when n is
                              0b11u32 -> n),
                    bu64:  (\n ->
                            when n is
                              0b11u64 -> n),
                    bu128: (\n ->
                            when n is
                              0b11u128 -> n),

                    bi8:   (\n ->
                            when n is
                              0b11i8 -> n),
                    bi16:  (\n ->
                            when n is
                              0b11i16 -> n),
                    bi32:  (\n ->
                            when n is
                              0b11i32 -> n),
                    bi64:  (\n ->
                            when n is
                              0b11i64 -> n),
                    bi128: (\n ->
                            when n is
                              0b11i128 -> n),

                    bnat:  (\n ->
                            when n is
                              0b11nat -> n),

                    dec:  (\n ->
                            when n is
                              123.0dec -> n),
                    f32:  (\n ->
                            when n is
                              123.0f32 -> n),
                    f64:  (\n ->
                            when n is
                              123.0f64 -> n),

                    fdec: (\n ->
                            when n is
                              123dec -> n),
                    ff32: (\n ->
                            when n is
                              123f32 -> n),
                    ff64: (\n ->
                            when n is
                              123f64 -> n),
                }
                "#
            ),
            r#"{ bi128 : I128 -> I128, bi16 : I16 -> I16, bi32 : I32 -> I32, bi64 : I64 -> I64, bi8 : I8 -> I8, bnat : Nat -> Nat, bu128 : U128 -> U128, bu16 : U16 -> U16, bu32 : U32 -> U32, bu64 : U64 -> U64, bu8 : U8 -> U8, dec : Dec -> Dec, f32 : F32 -> F32, f64 : F64 -> F64, fdec : Dec -> Dec, ff32 : F32 -> F32, ff64 : F64 -> F64, i128 : I128 -> I128, i16 : I16 -> I16, i32 : I32 -> I32, i64 : I64 -> I64, i8 : I8 -> I8, nat : Nat -> Nat, u128 : U128 -> U128, u16 : U16 -> U16, u32 : U32 -> U32, u64 : U64 -> U64, u8 : U8 -> U8 }"#,
        )
    }

    #[test]
    fn issue_2458() {
        infer_eq_without_problem(
            indoc!(
                r#"
                Foo a : [ Blah (Result (Bar a) { val: a }) ]
                Bar a : Foo a

                v : Bar U8
                v = Blah (Ok (Blah (Err { val: 1 })))

                v
                "#
            ),
            "Bar U8",
        )
    }

    // https://github.com/rtfeldman/roc/issues/2379
    #[test]
    fn copy_vars_referencing_copied_vars() {
        infer_eq_without_problem(
            indoc!(
                r#"
                Job : [ Job [ Command ] (List Job) ]

                job : Job

                job
                "#
            ),
            "Job",
        )
    }

    #[test]
    fn copy_vars_referencing_copied_vars_specialized() {
        infer_eq_without_problem(
            indoc!(
                r#"
                Job a : [ Job [ Command ] (Job a) (List (Job a)) a ]

                job : Job Str

                when job is
                    Job _ j lst _ ->
                        when j is
                            Job _ _ _ s ->
                                { j, lst, s }
                "#
            ),
            // TODO: this means that we're doing our job correctly, as now both `Job a`s have been
            // specialized to the same type, and the second destructuring proves the reified type
            // is `Job Str`. But we should just print the structure of the recursive type directly.
            // See https://github.com/rtfeldman/roc/issues/2513
            "{ j : a, lst : List a, s : Str }",
        )
    }

    #[test]
    fn to_int() {
        infer_eq_without_problem(
            indoc!(
                r#"
                {
                    toI8: Num.toI8,
                    toI16: Num.toI16,
                    toI32: Num.toI32,
                    toI64: Num.toI64,
                    toI128: Num.toI128,
                    toNat: Num.toNat,
                    toU8: Num.toU8,
                    toU16: Num.toU16,
                    toU32: Num.toU32,
                    toU64: Num.toU64,
                    toU128: Num.toU128,
                }
                "#
            ),
            r#"{ toI128 : Int * -> I128, toI16 : Int * -> I16, toI32 : Int * -> I32, toI64 : Int * -> I64, toI8 : Int * -> I8, toNat : Int * -> Nat, toU128 : Int * -> U128, toU16 : Int * -> U16, toU32 : Int * -> U32, toU64 : Int * -> U64, toU8 : Int * -> U8 }"#,
        )
    }

    #[test]
    fn to_float() {
        infer_eq_without_problem(
            indoc!(
                r#"
                {
                    toF32: Num.toF32,
                    toF64: Num.toF64,
                }
                "#
            ),
            r#"{ toF32 : Num * -> F32, toF64 : Num * -> F64 }"#,
        )
    }

    #[test]
    fn opaque_wrap_infer() {
        infer_eq_without_problem(
            indoc!(
                r#"
                Age := U32

                $Age 21
                "#
            ),
            r#"Age"#,
        )
    }

    #[test]
    fn opaque_wrap_check() {
        infer_eq_without_problem(
            indoc!(
                r#"
                Age := U32

                a : Age
                a = $Age 21

                a
                "#
            ),
            r#"Age"#,
        )
    }

    #[test]
    fn opaque_wrap_polymorphic_infer() {
        infer_eq_without_problem(
            indoc!(
                r#"
                Id n := [ Id U32 n ]

                $Id (Id 21 "sasha")
                "#
            ),
            r#"Id Str"#,
        )
    }

    #[test]
    fn opaque_wrap_polymorphic_check() {
        infer_eq_without_problem(
            indoc!(
                r#"
                Id n := [ Id U32 n ]

                a : Id Str
                a = $Id (Id 21 "sasha")

                a
                "#
            ),
            r#"Id Str"#,
        )
    }

    #[test]
    fn opaque_wrap_polymorphic_from_multiple_branches_infer() {
        infer_eq_without_problem(
            indoc!(
                r#"
                Id n := [ Id U32 n ]
                condition : Bool

                if condition
                then $Id (Id 21 (Y "sasha"))
                else $Id (Id 21 (Z "felix"))
                "#
            ),
            r#"Id [ Y Str, Z Str ]*"#,
        )
    }

    #[test]
    fn opaque_wrap_polymorphic_from_multiple_branches_check() {
        infer_eq_without_problem(
            indoc!(
                r#"
                Id n := [ Id U32 n ]
                condition : Bool

                v : Id [ Y Str, Z Str ]
                v =
                    if condition
                    then $Id (Id 21 (Y "sasha"))
                    else $Id (Id 21 (Z "felix"))

                v
                "#
            ),
            r#"Id [ Y Str, Z Str ]"#,
        )
    }

    #[test]
    fn opaque_unwrap_infer() {
        infer_eq_without_problem(
            indoc!(
                r#"
                Age := U32

                \$Age n -> n
                "#
            ),
            r#"Age -> U32"#,
        )
    }

    #[test]
    fn opaque_unwrap_check() {
        infer_eq_without_problem(
            indoc!(
                r#"
                Age := U32

                v : Age -> U32
                v = \$Age n -> n
                v
                "#
            ),
            r#"Age -> U32"#,
        )
    }

    #[test]
    fn opaque_unwrap_polymorphic_infer() {
        infer_eq_without_problem(
            indoc!(
                r#"
                Id n := [ Id U32 n ]

                \$Id (Id _ n) -> n
                "#
            ),
            r#"Id a -> a"#,
        )
    }

    #[test]
    fn opaque_unwrap_polymorphic_check() {
        infer_eq_without_problem(
            indoc!(
                r#"
                Id n := [ Id U32 n ]

                v : Id a -> a
                v = \$Id (Id _ n) -> n

                v
                "#
            ),
            r#"Id a -> a"#,
        )
    }

    #[test]
    fn opaque_unwrap_polymorphic_specialized_infer() {
        infer_eq_without_problem(
            indoc!(
                r#"
                Id n := [ Id U32 n ]

                strToBool : Str -> Bool

                \$Id (Id _ n) -> strToBool n
                "#
            ),
            r#"Id Str -> Bool"#,
        )
    }

    #[test]
    fn opaque_unwrap_polymorphic_specialized_check() {
        infer_eq_without_problem(
            indoc!(
                r#"
                Id n := [ Id U32 n ]

                strToBool : Str -> Bool

                v : Id Str -> Bool
                v = \$Id (Id _ n) -> strToBool n

                v
                "#
            ),
            r#"Id Str -> Bool"#,
        )
    }

    #[test]
    fn opaque_unwrap_polymorphic_from_multiple_branches_infer() {
        infer_eq_without_problem(
            indoc!(
                r#"
                Id n := [ Id U32 n ]

                \id ->
                    when id is
                        $Id (Id _ A) -> ""
                        $Id (Id _ B) -> ""
                        $Id (Id _ (C { a: "" })) -> ""
                "#
            ),
            r#"Id [ A, B, C { a : Str }* ] -> Str"#,
        )
    }

    #[test]
    fn opaque_unwrap_polymorphic_from_multiple_branches_check() {
        infer_eq_without_problem(
            indoc!(
                r#"
                Id n := [ Id U32 n ]

                f : Id [ A, B, C { a : Str }e ] -> Str
                f = \id ->
                    when id is
                        $Id (Id _ A) -> ""
                        $Id (Id _ B) -> ""
                        $Id (Id _ (C { a: "" })) -> ""

                f
                "#
            ),
            r#"Id [ A, B, C { a : Str }e ] -> Str"#,
        )
    }

    #[test]
    fn lambda_set_within_alias_is_quantified() {
        infer_eq_without_problem(
            indoc!(
                r#"
                app "test" provides [ effectAlways ] to "./platform"

                Effect a : [ @Effect ({} -> a) ]

                effectAlways : a -> Effect a
                effectAlways = \x ->
                    inner = \{} -> x

                    @Effect inner
                "#
            ),
            r#"a -> Effect a"#,
        )
    }

    #[test]
    fn generalized_accessor_function_applied() {
        infer_eq_without_problem(
            indoc!(
                r#"
                returnFoo = .foo

                returnFoo { foo: "foo" }
                "#
            ),
            "Str",
        )
    }

    #[test]
    fn record_extension_variable_is_alias() {
        infer_eq_without_problem(
            indoc!(
                r#"
                Other a b : { y: a, z: b }

                f : { x : Str }(Other Str Str)
                f
                "#
            ),
            r#"{ x : Str, y : Str, z : Str }"#,
        )
    }

    #[test]
    fn tag_extension_variable_is_alias() {
        infer_eq_without_problem(
            indoc!(
                r#"
                Other : [ B, C ]

                f : [ A ]Other
                f
                "#
            ),
            r#"[ A, B, C ]"#,
        )
    }

    #[test]
    // https://github.com/rtfeldman/roc/issues/2702
    fn tag_inclusion_behind_opaque() {
        infer_eq_without_problem(
            indoc!(
                r#"
                Outer k := [ Empty, Wrapped k ]

                insert : Outer k, k -> Outer k
                insert = \m, var ->
                    when m is
                        $Outer Empty -> $Outer (Wrapped var)
                        $Outer (Wrapped _) -> $Outer (Wrapped var)

                insert
                "#
            ),
            r#"Outer k, k -> Outer k"#,
        )
    }

    #[test]
    fn tag_inclusion_behind_opaque_infer() {
        infer_eq_without_problem(
            indoc!(
                r#"
                Outer k := [ Empty, Wrapped k ]

                when ($Outer Empty) is
                    $Outer Empty -> $Outer (Wrapped "")
                    $Outer (Wrapped k) -> $Outer (Wrapped k)
                "#
            ),
            r#"Outer Str"#,
        )
    }

    #[test]
    fn tag_inclusion_behind_opaque_infer_single_ctor() {
        infer_eq_without_problem(
            indoc!(
                r#"
                Outer := [ A, B ]

                when ($Outer A) is
                    $Outer A -> $Outer A
                    $Outer B -> $Outer B
                "#
            ),
            r#"Outer"#,
        )
    }

    #[test]
    fn issue_2583_specialize_errors_behind_unified_branches() {
        infer_eq_without_problem(
            indoc!(
                r#"
                if True then List.first [] else Str.toI64 ""
                "#
            ),
            "Result I64 [ InvalidNumStr, ListWasEmpty ]*",
        )
    }

    #[test]
    fn lots_of_type_variables() {
        infer_eq_without_problem(
            indoc!(
                r#"
                fun = \a,b,c,d,e,f,g,h,i,j,k,l,m,n,o,p,q,r,s,t,u,v,w,x,y,z,aa,bb -> {a,b,c,d,e,f,g,h,i,j,k,l,m,n,o,p,q,r,s,t,u,v,w,x,y,z,aa,bb}
                fun
                "#
            ),
            "a, b, c, d, e, f, g, h, i, j, k, l, m, n, o, p, q, r, s, t, u, v, w, x, y, z, aa, bb -> { a : a, aa : aa, b : b, bb : bb, c : c, d : d, e : e, f : f, g : g, h : h, i : i, j : j, k : k, l : l, m : m, n : n, o : o, p : p, q : q, r : r, s : s, t : t, u : u, v : v, w : w, x : x, y : y, z : z }",
        )
    }
}
