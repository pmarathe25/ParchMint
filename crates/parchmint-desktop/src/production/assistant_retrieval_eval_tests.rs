// A deterministic, hand-authored retrieval evaluation. These cases assess
// evidence selection only; they do not score a model's interpretation.

#[test]
fn story_corpus_reports_retrieval_recall_and_scope_separately() {
    struct Case {
        family: &'static str,
        name: &'static str,
        query: &'static str,
        target: &'static str,
        support: &'static str,
        qualifier: &'static str,
        interpretation: &'static str,
        forbidden: &'static str,
        exclude_support: bool,
    }
    let cases = [
        Case {
            family: "alias",
            name: "explicit name",
            query: "Who is Mara the Lantern Keeper?",
            target: "Mara guards the lantern gate.",
            support: "The Lantern Keeper is Mara.",
            qualifier: "Mara took the keeper's title after the siege.",
            interpretation: "identity",
            forbidden: "two separate people",
            exclude_support: false,
        },
        Case {
            family: "alias",
            name: "identity bridge",
            query: "Where is the Lantern Keeper?",
            target: "The Lantern Keeper is absent tonight.",
            support: "Mara waits beneath the bridge.",
            qualifier: "Mara is the Lantern Keeper.",
            interpretation: "identity requires bridge",
            forbidden: "invented location",
            exclude_support: false,
        },
        Case {
            family: "alias",
            name: "paraphrase",
            query: "Where did the night watch go?",
            target: "The night watch left before dawn.",
            support: "Eren waited beside the mill.",
            qualifier: "The night watch is Eren's nickname.",
            interpretation: "identity requires bridge",
            forbidden: "unsupported location",
            exclude_support: false,
        },
        Case {
            family: "alias",
            name: "shared nickname",
            query: "Which Fox entered the gate?",
            target: "The Fox entered the gate.",
            support: "Both Ivo and Neri use the name Fox.",
            qualifier: "No witness saw which Fox entered.",
            interpretation: "uncertain identity",
            forbidden: "definite identity",
            exclude_support: false,
        },
        Case {
            family: "flashback",
            name: "after siege",
            query: "Does Mara have both eyes after the siege?",
            target: "After the siege Mara watched with both eyes.",
            support: "Mara lost her left eye in the siege.",
            qualifier: "The siege ended before this scene.",
            interpretation: "possible conflict",
            forbidden: "certainty without chronology",
            exclude_support: false,
        },
        Case {
            family: "flashback",
            name: "before siege",
            query: "Did Mara have both eyes before the siege?",
            target: "Before the siege Mara watched with both eyes.",
            support: "Mara lost her left eye in the siege.",
            qualifier: "The scene occurs before the siege.",
            interpretation: "explained by time",
            forbidden: "definite contradiction",
            exclude_support: false,
        },
        Case {
            family: "flashback",
            name: "separate qualifier",
            query: "Does Mara's eye conflict with the siege?",
            target: "Mara watched the dancers with both eyes.",
            support: "Mara lost her left eye in the siege.",
            qualifier: "The dance was a flashback to before the siege.",
            interpretation: "explained by flashback",
            forbidden: "definite contradiction",
            exclude_support: false,
        },
        Case {
            family: "flashback",
            name: "unknown date",
            query: "Is Mara's eye consistent with the siege?",
            target: "Mara watched the sea with both eyes.",
            support: "Mara lost her left eye in the siege.",
            qualifier: "The sea scene has no established date.",
            interpretation: "uncertain chronology",
            forbidden: "definite contradiction",
            exclude_support: false,
        },
        Case {
            family: "belief",
            name: "narrated conflict",
            query: "Who owns Ivo's silver key?",
            target: "Ivo owns the silver key.",
            support: "Neri owns the silver key.",
            qualifier: "No key transfer has been established.",
            interpretation: "possible conflict",
            forbidden: "invented transfer",
            exclude_support: false,
        },
        Case {
            family: "belief",
            name: "rumor",
            query: "Is the harbor gate sealed?",
            target: "Villagers say the harbor gate is sealed.",
            support: "The harbor gate stood open at dawn.",
            qualifier: "The claim about the gate is a rumor.",
            interpretation: "rumor versus observation",
            forbidden: "definite contradiction",
            exclude_support: false,
        },
        Case {
            family: "belief",
            name: "mistaken belief",
            query: "What does Ivo believe about the key?",
            target: "Ivo believes Neri took the key.",
            support: "Mara took the key while Ivo slept.",
            qualifier: "Ivo later learns his belief was mistaken.",
            interpretation: "belief corrected",
            forbidden: "objective inconsistency",
            exclude_support: false,
        },
        Case {
            family: "belief",
            name: "unclear speaker",
            query: "Who said the bridge fell?",
            target: "Someone said the bridge fell.",
            support: "Neri saw the bridge standing.",
            qualifier: "The speaker and time of the claim are unknown.",
            interpretation: "uncertain attribution",
            forbidden: "invented speaker",
            exclude_support: false,
        },
        Case {
            family: "outside",
            name: "other group",
            query: "Who holds the copper seal?",
            target: "Mara holds the copper seal.",
            support: "Ivo holds the copper seal.",
            qualifier: "No transfer of the copper seal is recorded.",
            interpretation: "possible conflict",
            forbidden: "invented transfer",
            exclude_support: false,
        },
        Case {
            family: "outside",
            name: "lore rule",
            query: "Can the moon gate open at noon?",
            target: "The moon gate opens at noon.",
            support: "The moon gate opens only at night.",
            qualifier: "No exception to the moon gate rule is recorded.",
            interpretation: "possible conflict",
            forbidden: "invented exception",
            exclude_support: false,
        },
        Case {
            family: "outside",
            name: "excluded evidence",
            query: "Who holds the amber ring?",
            target: "Mara holds the amber ring.",
            support: "Ivo holds the amber ring.",
            qualifier: "No ring transfer is recorded.",
            interpretation: "insufficient permitted evidence",
            forbidden: "use excluded source",
            exclude_support: true,
        },
        Case {
            family: "outside",
            name: "crowded context",
            query: "Which keeper has the silver token?",
            target: "The keeper gives Neri the silver token.",
            support: "The keeper gave Ivo the silver token.",
            qualifier: "No silver token transfer is recorded.",
            interpretation: "possible conflict",
            forbidden: "claim exhaustive coverage",
            exclude_support: false,
        },
        Case {
            family: "holdout",
            name: "direct compass",
            query: "Where is Talia's brass compass?",
            target: "Talia carries the brass compass.",
            support: "Talia left the brass compass on the desk.",
            qualifier: "No brass compass transfer is recorded.",
            interpretation: "possible conflict",
            forbidden: "invented transfer",
            exclude_support: false,
        },
        Case {
            family: "holdout",
            name: "direct oath",
            query: "When was Neri's midnight oath?",
            target: "Neri made the midnight oath after winter.",
            support: "Neri made the midnight oath before winter.",
            qualifier: "There is only one midnight oath.",
            interpretation: "possible conflict",
            forbidden: "invented second oath",
            exclude_support: false,
        },
        Case {
            family: "holdout",
            name: "star warden alias",
            query: "Where is the Star Warden?",
            target: "The Star Warden crossed the ridge.",
            support: "Lysa waits beside the cedar.",
            qualifier: "Lysa is the Star Warden.",
            interpretation: "identity requires bridge",
            forbidden: "invented identity",
            exclude_support: false,
        },
        Case {
            family: "holdout",
            name: "shared title",
            query: "Which Raven opened the gate?",
            target: "The Raven opened the gate.",
            support: "Both Talia and Lysa use the title Raven.",
            qualifier: "No witness identified the Raven at the gate.",
            interpretation: "uncertain identity",
            forbidden: "definite identity",
            exclude_support: false,
        },
        Case {
            family: "scope",
            name: "excluded identity bridge",
            query: "Where is the Snow Warden?",
            target: "The Snow Warden vanished before dawn.",
            support: "Yara waits beside the pines.",
            qualifier: "Yara is the Snow Warden.",
            interpretation: "insufficient permitted evidence",
            forbidden: "use excluded identity",
            exclude_support: false,
        },
        Case {
            family: "holdout",
            name: "long target",
            query: "What happened at the crossing?",
            target: "The crossing left Zora's sapphire compass unclaimed.",
            support: "Zora carried the sapphire compass in secret.",
            qualifier: "The crossing's sapphire compass had no recorded transfer.",
            interpretation: "possible conflict",
            forbidden: "claim complete coverage",
            exclude_support: false,
        },
        Case {
            family: "holdout",
            name: "short character name",
            query: "Where is Li?",
            target: "Li crossed the ridge.",
            support: "Li waits beside the cedar.",
            qualifier: "Li's journey happened before the storm.",
            interpretation: "location depends on time",
            forbidden: "invented chronology",
            exclude_support: false,
        },
    ];
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}/v1", listener.local_addr().unwrap());
    let case_count = cases.len();
    let server = std::thread::spawn(move || {
        for _ in 0..case_count {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            let mut bytes = Vec::new();
            let mut buffer = [0; 4096];
            let header_end = loop {
                let count = stream.read(&mut buffer).unwrap();
                assert!(count > 0);
                bytes.extend_from_slice(&buffer[..count]);
                if let Some(index) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
                    break index + 4;
                }
            };
            let header = std::str::from_utf8(&bytes[..header_end]).unwrap();
            let length = header
                .lines()
                .find_map(|line| {
                    line.split_once(':')
                        .filter(|(name, _)| name.eq_ignore_ascii_case("content-length"))
                        .and_then(|(_, value)| value.trim().parse::<usize>().ok())
                })
                .unwrap();
            while bytes.len() < header_end + length {
                let count = stream.read(&mut buffer).unwrap();
                assert!(count > 0);
                bytes.extend_from_slice(&buffer[..count]);
            }
            let answer =
                serde_json::json!({"summary":"Offline retrieval evaluation.","findings":[]})
                    .to_string();
            let body = serde_json::to_vec(&serde_json::json!({"choices":[{"finish_reason":"stop","message":{"content":answer}}],"usage":{"prompt_tokens":1,"completion_tokens":1}})).unwrap();
            write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).unwrap();
            stream.write_all(&body).unwrap();
        }
    });
    let mut project = Project::new(ProjectId::from_bytes([1; 16]));
    let mut documents = Vec::new();
    project
        .nodes
        .try_insert_group(
            NodeId::from_bytes([211; 16]),
            ProjectSection::Manuscript.root_id(),
            0,
            "Other group",
        )
        .unwrap();
    project
        .nodes
        .try_insert_group(
            NodeId::from_bytes([213; 16]),
            ProjectSection::Lore.root_id(),
            0,
            "Excluded group",
        )
        .unwrap();
    for (index, case) in cases.iter().enumerate() {
        let target_group = if index >= 12 {
            let group = NodeId::from_bytes([u8::try_from(100 + index).unwrap(); 16]);
            project
                .nodes
                .try_insert_group(
                    group,
                    ProjectSection::Manuscript.root_id(),
                    0,
                    format!("Group {index}"),
                )
                .unwrap();
            Some(group)
        } else {
            None
        };
        for (offset, text) in [case.target, case.support, case.qualifier]
            .into_iter()
            .enumerate()
        {
            let id = u8::try_from(20 + index * 3 + offset).unwrap();
            let document = DocumentId::from_bytes([id; 16]);
            let section = if offset == 0 || (index == 12 && offset == 1) {
                ProjectSection::Manuscript
            } else {
                ProjectSection::Lore
            };
            let parent = if offset == 0 {
                target_group.unwrap_or_else(|| section.root_id())
            } else if index == 12 && offset == 1 {
                NodeId::from_bytes([211; 16])
            } else if case.exclude_support && offset == 1 {
                NodeId::from_bytes([213; 16])
            } else {
                section.root_id()
            };
            project
                .nodes
                .try_insert_document(
                    NodeId::from_bytes([id; 16]),
                    document,
                    parent,
                    0,
                    format!("Source {id}"),
                )
                .unwrap();
            let body = if case.name == "long target" && offset == 0 {
                format!(
                    "{}<p>{text}</p>",
                    (0..10)
                        .map(|i| format!("<p>Crossing record {i}: {}.</p>", "detail ".repeat(145)))
                        .collect::<String>()
                )
            } else {
                format!("<p>{text}</p>")
            };
            documents.push(DocumentSnapshot {
                document_id: document,
                body,
                comments: Vec::new(),
                revision: EditorRevision::from(1),
                visibility: DocumentVisibility::Closed,
            });
        }
    }
    // The outside-group case checks two documents within the target group.
    let second_target = DocumentId::from_bytes([212; 16]);
    project
        .nodes
        .try_insert_document(
            NodeId::from_bytes([212; 16]),
            second_target,
            NodeId::from_bytes([112; 16]),
            0,
            "Another source",
        )
        .unwrap();
    documents.push(DocumentSnapshot {
        document_id: second_target,
        body: "<p>The copper seal was later seen in the vault.</p>".into(),
        comments: Vec::new(),
        revision: EditorRevision::from(1),
        visibility: DocumentVisibility::Closed,
    });
    // Repeated relevant-looking terms compete for the bounded context budget.
    let noise = DocumentId::from_bytes([180; 16]);
    project
        .nodes
        .try_insert_document(
            NodeId::from_bytes([180; 16]),
            noise,
            ProjectSection::Manuscript.root_id(),
            0,
            "Archive",
        )
        .unwrap();
    documents.push(DocumentSnapshot {
        document_id: noise,
        body: (0..80)
            .map(|index| {
                format!("<p>The keeper recorded silver token details in ledger {index}.</p>")
            })
            .collect(),
        comments: Vec::new(),
        revision: EditorRevision::from(1),
        visibility: DocumentVisibility::Closed,
    });
    let query = Arc::new(SnapshotQuery(
        ProjectSnapshot {
            project,
            document_summaries: Vec::new(),
            documents,
            styles_css: String::new(),
        },
        std::collections::BTreeMap::new(),
    ));
    let path = std::env::temp_dir().join(format!(
        "parchmint-retrieval-eval-{}-{}.json",
        std::process::id(),
        listener_port(&endpoint)
    ));
    let preferences: Arc<dyn PreferenceService> = Arc::new(PreferenceCoordinator::new(Arc::new(
        FilePreferenceStore::new(&path),
    )));
    let current = block_on(preferences.load()).unwrap();
    block_on(preferences.update(
        current.revision,
        PreferenceCommand::SetAssistant(AssistantPreferences {
            provider: AssistantProvider::OpenAiCompatible,
            endpoint,
            model: "offline-eval".into(),
            authentication: AssistantAuthentication::None,
            ..Default::default()
        }),
    ))
    .unwrap();
    let fixture = NativeFixture::new();
    let platform = PlatformServices::new(
        fixture.menus(),
        fixture.dialogs(),
        fixture.clipboard(),
        fixture.external_open(),
        fixture.application_paths(),
        fixture.appearance(),
    )
    .with_session_credentials(Arc::new(SessionCredentialStore::default()));
    let assistant = ProductionAssistant {
        query,
        preferences,
        platform,
        budget: None,
        conversations: None,
    };
    let mut families: std::collections::BTreeMap<&str, (usize, usize, usize, usize)> =
        std::collections::BTreeMap::new();
    let mut misses = Vec::new();
    let mut truncated = 0;
    let mut unanswerable_under_scope = 0;
    let mut outside_recall = (0_usize, 0_usize);
    let mut qualifier_recall = (0_usize, 0_usize);
    let mut gained = Vec::new();
    let mut displaced = Vec::new();
    let baseline_snapshot = assistant.query.snapshot_with_documents().unwrap();
    let baseline_snapshot = AuthoredProjectSnapshot {
        project: baseline_snapshot.project,
        document_summaries: Vec::new(),
        documents: baseline_snapshot.documents,
    };
    for (index, case) in cases.iter().enumerate() {
        let target_document = NodeId::from_bytes([u8::try_from(20 + index * 3).unwrap(); 16]);
        let target = if index >= 12 {
            NodeId::from_bytes([u8::try_from(100 + index).unwrap(); 16])
        } else {
            target_document
        };
        let support = NodeId::from_bytes([u8::try_from(21 + index * 3).unwrap(); 16]);
        let qualifier = NodeId::from_bytes([u8::try_from(22 + index * 3).unwrap(); 16]);
        let exclude_bridge = case.name == "excluded identity bridge";
        let request = AssistantWorkflowRequest {
            mode: AssistantMode::Check,
            prompt: case.query.into(),
            target: Some(target),
            context_scope: None,
            selected_text: None,
            pinned_context: Vec::new(),
            excluded_context: if case.exclude_support {
                vec![NodeId::from_bytes([213; 16])]
            } else if exclude_bridge {
                vec![qualifier]
            } else {
                vec![]
            },
            history: Vec::new(),
            include_notes: false,
            explicit_notes: None,
            create_note: false,
            live_documents: Vec::new(),
            edit_proposal: None,
        };
        let result = assistant.run(request, &AtomicBool::new(false)).unwrap();
        assert!(result.context.len() <= parchmint_assistant_core::MAX_PASSAGES);
        assert!(
            result.context.iter().map(|p| p.text.len()).sum::<usize>()
                <= parchmint_assistant_core::MAX_TOTAL_BYTES
        );
        let contains = |id: NodeId, text: &str| {
            result.context.iter().any(|passage| {
                passage.document_id == parchmint_domain::encode_stable_id(id.as_bytes())
                    && passage.text.contains(text)
            })
        };
        let target_found = contains(target_document, case.target);
        if case.name == "other group" {
            assert!(
                result.context.iter().any(|passage| passage.document_id
                    == parchmint_domain::encode_stable_id(second_target.as_bytes())
                    && passage.text.contains("copper seal")),
                "second target-group document omitted"
            );
        }
        let support_found = contains(support, case.support);
        let qualifier_found = contains(qualifier, case.qualifier);
        if case.name == "crowded context" {
            assert!(qualifier_found, "repeated passages crowded out the qualifier");
        }
        if case.exclude_support {
            assert!(
                result.context.iter().all(|passage| passage.document_id
                    != parchmint_domain::encode_stable_id(support.as_bytes())),
                "excluded source leaked: {}",
                case.name
            );
        }
        if exclude_bridge {
            assert!(
                result.context.iter().all(|passage| passage.document_id
                    != parchmint_domain::encode_stable_id(qualifier.as_bytes())),
                "excluded identity leaked"
            );
            assert!(!support_found, "excluded identity influenced expansion");
        }
        let required = if exclude_bridge {
            1
        } else if case.exclude_support {
            2
        } else {
            3
        };
        let found = usize::from(target_found)
            + usize::from(qualifier_found && !exclude_bridge)
            + usize::from(support_found && !case.exclude_support && !exclude_bridge);
        let bundle = found == required;
        if !case.exclude_support && !exclude_bridge {
            outside_recall.0 += usize::from(support_found);
            outside_recall.1 += 1;
        }
        if !exclude_bridge {
            qualifier_recall.0 += usize::from(qualifier_found);
            qualifier_recall.1 += 1;
        }
        // Compare against the prior production policy: original question plus
        // bounded target-derived terms, without identity-name reservation.
        let pinned = target_documents(&baseline_snapshot.project, target).unwrap();
        let excluded = if case.exclude_support {
            BTreeSet::from([DocumentId::from_bytes(*support.as_bytes())])
        } else if exclude_bridge {
            BTreeSet::from([DocumentId::from_bytes(*qualifier.as_bytes())])
        } else {
            BTreeSet::new()
        };
        let initial = parchmint_assistant_core::retrieve_with_originals(
            &baseline_snapshot,
            case.query,
            &pinned,
            &excluded,
            &[],
        )
        .unwrap();
        let mut expansion = case.query.to_owned();
        for passage in initial.passages.iter().filter(|passage| {
            pinned
                .iter()
                .any(|id| passage.document_id == parchmint_domain::encode_stable_id(id.as_bytes()))
        }) {
            if expansion.len() >= 16 * 1024 {
                break;
            }
            expansion.push(' ');
            expansion.push_str(&passage.text);
        }
        let baseline = parchmint_assistant_core::retrieve_with_originals_and_expansion(
            &baseline_snapshot,
            case.query,
            &expansion,
            &pinned,
            &excluded,
            &[],
        )
        .unwrap();
        let old_contains = |id: NodeId, text: &str| {
            baseline.passages.iter().any(|passage| {
                passage.document_id == parchmint_domain::encode_stable_id(id.as_bytes())
                    && passage.text.contains(text)
            })
        };
        for (label, old, new) in [
            (
                "target",
                old_contains(target_document, case.target),
                target_found,
            ),
            (
                "support",
                !case.exclude_support && !exclude_bridge && old_contains(support, case.support),
                !case.exclude_support && !exclude_bridge && support_found,
            ),
            (
                "qualifier",
                !exclude_bridge && old_contains(qualifier, case.qualifier),
                !exclude_bridge && qualifier_found,
            ),
        ] {
            if old && !new {
                displaced.push(format!("{} / {} / {label}", case.family, case.name));
            }
            if !old && new {
                gained.push(format!("{} / {} / {label}", case.family, case.name));
            }
        }
        let metrics = families.entry(case.family).or_default();
        metrics.0 += found;
        metrics.1 += required;
        if case.exclude_support || exclude_bridge {
            unanswerable_under_scope += 1;
        } else {
            metrics.2 += usize::from(bundle);
            metrics.3 += 1;
        }
        if !bundle {
            misses.push(format!(
                "{} / {} ({}/{required}; expected {}; avoid {})",
                case.family, case.name, found, case.interpretation, case.forbidden
            ));
        }
        truncated += usize::from(result.context_truncated);
        assert!(target_found, "target omitted: {}", case.name);
    }
    for (family, (found, required, bundles, count)) in &families {
        eprintln!(
            "{family}: permitted evidence {found}/{required}; complete answerable bundles {bundles}/{count}"
        );
    }
    eprintln!(
        "Outside-target evidence: {}/{}; qualifiers: {}/{}; unanswerable under scope: {unanswerable_under_scope}",
        outside_recall.0, outside_recall.1, qualifier_recall.0, qualifier_recall.1
    );
    eprintln!(
        "Truncated requests: {truncated}/{}; incomplete cases: {misses:?}",
        cases.len()
    );
    eprintln!("Gained gold passages: {gained:?}; displaced gold passages: {displaced:?}");
    eprintln!(
        "Answer truth, unsupported claims, and interpretation are not measured by this retrieval evaluation."
    );
    assert_eq!(families.len(), 6);
    assert!(
        displaced.is_empty(),
        "gold evidence displaced: {displaced:?}"
    );
    server.join().unwrap();
    std::fs::remove_file(path).unwrap();
}
