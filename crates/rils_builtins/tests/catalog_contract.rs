use rils_builtins::{
    BUILTINS, BuiltinKind, BuiltinMemberKind, FLOAT_INTRINSICS, INTEGER_INTRINSICS,
    RuntimeMemberId, TypePattern, builtin, builtin_member, runtime_member,
};

#[test]
fn declarations_have_unique_stable_identity_and_complete_metadata() {
    let intrinsics = INTEGER_INTRINSICS
        .iter()
        .chain(FLOAT_INTRINSICS)
        .collect::<Vec<_>>();
    for (index, left) in intrinsics.iter().enumerate() {
        assert!(
            intrinsics[index + 1..]
                .iter()
                .all(|right| left.id != right.id)
        );
    }
    for declarations in [INTEGER_INTRINSICS, FLOAT_INTRINSICS] {
        for (index, left) in declarations.iter().enumerate() {
            assert!(
                declarations[index + 1..]
                    .iter()
                    .all(|right| left.kind != right.kind || left.name != right.name)
            );
        }
    }
    for (index, declaration) in BUILTINS.iter().enumerate() {
        assert!(
            BUILTINS[index + 1..]
                .iter()
                .all(|other| declaration.path != other.path)
        );
        assert!(!declaration.documentation.is_empty());
        for (member_index, member) in declaration.members.iter().enumerate() {
            assert!(!member.documentation.is_empty());
            assert!(
                declaration.members[member_index + 1..]
                    .iter()
                    .all(|other| member.name != other.name)
            );
            if member.kind == BuiltinMemberKind::Method {
                assert!(member.signature.is_some());
                assert!(member.receiver.is_some());
            }
        }
    }
}

#[test]
fn overloaded_runtime_methods_use_owner_qualified_imports() {
    for declaration in BUILTINS {
        for member in declaration.members {
            let Some(import) = member.runtime.and_then(RuntimeMemberId::bytecode_import) else {
                continue;
            };
            for other in BUILTINS
                .iter()
                .flat_map(|declaration| declaration.members)
                .filter(|other| other.name == member.name)
            {
                if let Some(other_import) = other.runtime.and_then(RuntimeMemberId::bytecode_import)
                    && import != other_import
                {
                    assert!(import.starts_with("core::") && other_import.starts_with("core::"));
                }
            }
        }
    }
}

#[test]
fn runtime_member_catalog_is_bidirectional_at_its_boundaries() {
    for declaration in BUILTINS {
        for member in declaration.members {
            if let Some(id) = member.runtime {
                let (_, found) = runtime_member(id).expect("runtime member declaration");
                assert_eq!(found.runtime, Some(id));
            }
        }
    }
    for id in [
        RuntimeMemberId::Clone,
        RuntimeMemberId::SequenceLen,
        RuntimeMemberId::VecPush,
        RuntimeMemberId::VecPop,
        RuntimeMemberId::SequenceIsEmpty,
        RuntimeMemberId::VecClear,
        RuntimeMemberId::VecTruncate,
        RuntimeMemberId::SequenceContains,
        RuntimeMemberId::VecInsert,
        RuntimeMemberId::VecRemove,
        RuntimeMemberId::VecSwapRemove,
        RuntimeMemberId::VecExtend,
        RuntimeMemberId::IteratorCount,
        RuntimeMemberId::IteratorLast,
        RuntimeMemberId::IteratorNth,
        RuntimeMemberId::IteratorCollectVec,
        RuntimeMemberId::IteratorTake,
        RuntimeMemberId::IteratorSkip,
        RuntimeMemberId::IteratorRev,
        RuntimeMemberId::IteratorIntoIter,
        RuntimeMemberId::IteratorMap,
        RuntimeMemberId::IteratorFilter,
        RuntimeMemberId::IteratorFilterMap,
        RuntimeMemberId::IteratorFold,
        RuntimeMemberId::IteratorForEach,
        RuntimeMemberId::IteratorAny,
        RuntimeMemberId::IteratorAll,
        RuntimeMemberId::IteratorFind,
        RuntimeMemberId::IteratorPosition,
        RuntimeMemberId::IteratorEnumerate,
        RuntimeMemberId::SequenceIntoIter,
        RuntimeMemberId::IteratorNext,
        RuntimeMemberId::RangeNext,
        RuntimeMemberId::RangeIntoIter,
        RuntimeMemberId::ResultIsOk,
        RuntimeMemberId::ResultIsErr,
        RuntimeMemberId::ResultUnwrap,
        RuntimeMemberId::ResultUnwrapOr,
        RuntimeMemberId::ResultExpect,
        RuntimeMemberId::ResultOk,
        RuntimeMemberId::ResultErr,
        RuntimeMemberId::ResultUnwrapErr,
        RuntimeMemberId::ResultExpectErr,
        RuntimeMemberId::ResultMap,
        RuntimeMemberId::ResultMapErr,
        RuntimeMemberId::ResultAndThen,
        RuntimeMemberId::ResultOrElse,
        RuntimeMemberId::OptionIsSome,
        RuntimeMemberId::OptionIsNone,
        RuntimeMemberId::OptionUnwrap,
        RuntimeMemberId::OptionUnwrapOr,
        RuntimeMemberId::OptionExpect,
        RuntimeMemberId::OptionTake,
        RuntimeMemberId::OptionOr,
        RuntimeMemberId::OptionXor,
        RuntimeMemberId::OptionReplace,
        RuntimeMemberId::OptionMap,
        RuntimeMemberId::OptionAndThen,
        RuntimeMemberId::OptionOrElse,
        RuntimeMemberId::StringLen,
        RuntimeMemberId::StringIsEmpty,
        RuntimeMemberId::StringContains,
        RuntimeMemberId::StringStartsWith,
        RuntimeMemberId::StringEndsWith,
        RuntimeMemberId::StringFind,
        RuntimeMemberId::StringTrim,
        RuntimeMemberId::StringReplace,
        RuntimeMemberId::StringTrimStart,
        RuntimeMemberId::StringTrimEnd,
        RuntimeMemberId::StringToLowercase,
        RuntimeMemberId::StringToUppercase,
        RuntimeMemberId::StringRepeat,
        RuntimeMemberId::StringRfind,
        RuntimeMemberId::StringStripPrefix,
        RuntimeMemberId::StringStripSuffix,
        RuntimeMemberId::StringChars,
        RuntimeMemberId::StringBytes,
        RuntimeMemberId::StringLines,
        RuntimeMemberId::StringSplit,
        RuntimeMemberId::HashMapLen,
        RuntimeMemberId::HashMapIsEmpty,
        RuntimeMemberId::HashMapClear,
        RuntimeMemberId::HashMapContainsKey,
        RuntimeMemberId::HashMapInsert,
        RuntimeMemberId::HashMapGetCloned,
        RuntimeMemberId::HashMapRemove,
        RuntimeMemberId::HashMapKeysCloned,
        RuntimeMemberId::HashMapValuesCloned,
        RuntimeMemberId::HashMapIntoIter,
        RuntimeMemberId::HashSetLen,
        RuntimeMemberId::HashSetIsEmpty,
        RuntimeMemberId::HashSetClear,
        RuntimeMemberId::HashSetContains,
        RuntimeMemberId::HashSetInsert,
        RuntimeMemberId::HashSetRemove,
        RuntimeMemberId::HashSetIsSubset,
        RuntimeMemberId::HashSetIsSuperset,
        RuntimeMemberId::HashSetIsDisjoint,
        RuntimeMemberId::HashSetUnion,
        RuntimeMemberId::HashSetIntersection,
        RuntimeMemberId::HashSetDifference,
        RuntimeMemberId::HashSetSymmetricDifference,
        RuntimeMemberId::HashSetIntoIter,
        RuntimeMemberId::FormatterWriteDerivedDebug,
    ] {
        assert!(
            runtime_member(id).is_some(),
            "missing runtime member {id:?}"
        );
    }
}

#[test]
fn default_trait_has_a_catalog_defined_associated_function() {
    let declaration = builtin("Default").expect("Default trait declaration");
    assert_eq!(declaration.kind, BuiltinKind::Trait);
    let member = builtin_member("Default", "default").expect("Default::default declaration");
    assert_eq!(member.kind, BuiltinMemberKind::AssociatedFunction);
    let signature = member.signature.expect("Default::default signature");
    assert!(signature.parameters.is_empty());
    assert_eq!(signature.result, TypePattern::SelfType);
}
