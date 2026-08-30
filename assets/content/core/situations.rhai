// Continuing political and strategic problems. Situations only read the
// semantic world view and return presentation data; every action below opens
// an ordinary authoritative assignment.

define_situation(#{
    id: "planetary-succession",
    source: "title",
    trigger_fn: "planetary_succession_instances",
    projection_fn: "planetary_succession_projection",
    priority: 100,
    stages: ["unclaimed", "contested", "pressing", "settled"],
    actions: [
        #{ id: "declare-claim", assignment: "declare-paramount-claim" },
        #{ id: "renounce-claim", assignment: "renounce-paramount-claim" },
        #{ id: "press-claim", assignment: "press-claim" },
        #{ id: "challenge-claimant", assignment: "declare-formal-war" },
    ],
    outcomes: [
        #{ id: "paramount-appointed", when_fn: "paramount_was_appointed" },
        #{ id: "closed", fallback: true },
    ],
});

fn planetary_succession_instances(ctx) {
    for title in ctx.world.titles {
        if title.id == ctx.source.id && title.holder_kind == "vacant" {
            return [#{}];
        }
    }
    []
}

fn planetary_succession_projection(ctx) {
    let stage = "unclaimed";
    let warning = false;
    let participants = [];
    let links = [];
    let actions = [];
    let values = #{};
    let pressing = false;
    let press_attempts = 0;
    let claimant_orgs = [];
    let metrics = [
        #{ label_key: "situation.metric.claimants", value: ctx.world.paramount_claims.len },
        #{ label_key: "situation.metric.realms", value: ctx.world.realm_totals.len },
    ];

    for title in ctx.world.titles {
        if title.id == ctx.source.id && title.holder_kind == "character" {
            stage = "settled";
            values["holder"] = character_name(ctx.world, title.holder);
        }
    }

    for work in ctx.world.assignments {
        if work.key == "press-claim" {
            pressing = true;
            press_attempts += 1;
            links.push(#{ kind: "assignment", id: work.id });
        }
    }
    if stage != "settled" {
        if pressing {
            stage = "pressing";
        } else if ctx.world.paramount_claims.len > 0 {
            stage = "contested";
        }
    }

    for claim in ctx.world.paramount_claims {
        participants.push(#{ kind: "character", id: claim.character });
        claimant_orgs.push(claim.organisation);
        if claim.can_press && claim.organisation != ctx.world.player_org {
            warning = true;
        }
        let provinces = 0;
        let army_manpower = 0;
        let legitimacy = 0;
        for realm in ctx.world.realm_totals {
            if realm.organisation == claim.organisation {
                provinces = realm.provinces;
                army_manpower = realm.army_manpower;
            }
        }
        for organisation in ctx.world.organisations {
            if organisation.id == claim.organisation {
                legitimacy = organisation.effective_legitimacy;
            }
        }
        let name = organisation_name(ctx.world, claim.organisation);
        metrics.push(#{
            label_key: "situation.metric.claimant-realm",
            value: `${name} — ${provinces} / ${army_manpower} / ${legitimacy}`,
        });
    }

    let claimant_wars = 0;
    for war in ctx.world.wars {
        if war.active {
            let first = false;
            let second = false;
            for claimant in claimant_orgs {
                if war.sides[0].members.contains(claimant) { first = true; }
                if war.sides[1].members.contains(claimant) { second = true; }
            }
            if first && second {
                claimant_wars += 1;
                links.push(#{ kind: "war", id: war.id });
            }
        }
    }
    metrics.push(#{ label_key: "situation.metric.claimant-wars", value: claimant_wars });
    metrics.push(#{ label_key: "situation.metric.press-attempts", value: press_attempts });

    let player_head = 0;
    for organisation in ctx.world.organisations {
        if organisation.id == ctx.world.player_org {
            player_head = organisation.head;
        }
    }
    if player_head > 0 {
        let player_claims = false;
        for claim in ctx.world.paramount_claims {
            if claim.character == player_head {
                player_claims = true;
            }
        }
        if player_claims {
            actions.push(#{ id: "renounce-claim", leader: player_head });
            actions.push(#{ id: "press-claim", leader: player_head });
            for claim in ctx.world.paramount_claims {
                if claim.character != player_head {
                    actions.push(#{
                        id: "challenge-claimant", leader: player_head,
                        target_kind: "organisation", target_a: claim.organisation,
                    });
                    break;
                }
            }
        } else {
            // The forecast remains visible when a liege or another rule makes
            // this unavailable, so the player learns what must change.
            actions.push(#{ id: "declare-claim", leader: player_head });
        }
    }

    #{
        stage: stage,
        warning: warning,
        metrics: metrics,
        participants: participants,
        links: links,
        actions: actions,
        resolution_values: values,
    }
}

fn paramount_was_appointed(ctx) {
    for title in ctx.world.titles {
        if title.id == ctx.source.id && title.holder_kind == "character" {
            return true;
        }
    }
    false
}

define_situation(#{
    id: "consular-vacancy",
    source: "title",
    trigger_fn: "consular_vacancy_instances",
    projection_fn: "consular_vacancy_projection",
    priority: 80,
    stages: ["open", "final-days", "appointed"],
    actions: [
        #{ id: "seek-favour", assignment: "curry-favour" },
        #{ id: "discredit-candidate", assignment: "discredit" },
    ],
    outcomes: [
        #{ id: "consul-appointed", when_fn: "consul_was_appointed" },
        #{ id: "closed", fallback: true },
    ],
});

fn consular_vacancy_instances(ctx) {
    for title in ctx.world.titles {
        if title.id == ctx.source.id && title.holder_kind == "vacant" {
            return [#{}];
        }
    }
    []
}

fn consular_vacancy_projection(ctx) {
    let stage = "open";
    let warning = false;
    let deadline = ctx.world.date;
    let participants = [];
    let actions = [];
    let values = #{};
    let metrics = [];

    for title in ctx.world.titles {
        if title.id == ctx.source.id && title.holder_kind == "character" {
            stage = "appointed";
            values["holder"] = character_name(ctx.world, title.holder);
        }
    }

    if type_of(ctx.world.consul_contest) == "map" {
        deadline = ctx.world.consul_contest.deadline;
        if deadline - ctx.world.date <= 30 {
            stage = "final-days";
            warning = true;
        }
        for score in ctx.world.consul_contest.scores {
            participants.push(#{ kind: "character", id: score.character });
            let name = character_name(ctx.world, score.character);
            metrics.push(#{
                label_key: "situation.metric.consul-score",
                value: `${name} — ${score.score}`,
            });
        }
    }
    metrics.push(#{ label_key: "situation.metric.candidates", value: participants.len });

    let player_head = 0;
    for organisation in ctx.world.organisations {
        if organisation.id == ctx.world.player_org {
            player_head = organisation.head;
        }
    }
    if player_head > 0 && stage != "appointed" {
        actions.push(#{ id: "seek-favour", leader: player_head });
        if participants.len > 0 {
            actions.push(#{
                id: "discredit-candidate", leader: player_head,
                target_kind: "character", target_a: participants[0].id,
            });
        }
    }

    #{
        stage: stage,
        deadline: deadline,
        warning: warning,
        metrics: metrics,
        participants: participants,
        actions: actions,
        resolution_values: values,
    }
}

fn consul_was_appointed(ctx) {
    for title in ctx.world.titles {
        if title.id == ctx.source.id && title.holder_kind == "character" {
            return true;
        }
    }
    false
}

define_situation(#{
    id: "favour-debt",
    source: "obligation",
    bindings: #{ debtor: "organisation", creditor: "organisation" },
    trigger_fn: "favour_debt_instances",
    projection_fn: "favour_debt_projection",
    priority: 60,
    audience: ["debtor", "creditor"],
    stages: ["owed", "honoured", "repudiated", "closed"],
    actions: [#{ id: "call-favour", assignment: "call-in-favour" }],
    outcomes: [
        #{ id: "honoured", when_fn: "favour_was_honoured" },
        #{ id: "repudiated", when_fn: "favour_was_repudiated" },
        #{ id: "closed", fallback: true },
    ],
});

fn favour_debt_instances(ctx) {
    for debt in ctx.world.obligations {
        if debt.id == ctx.source.id && debt.status == "open" {
            return [#{ debtor: debt.debtor, creditor: debt.creditor }];
        }
    }
    []
}

fn favour_debt_projection(ctx) {
    let stage = "closed";
    let weight = 0;
    for debt in ctx.world.obligations {
        if debt.id == ctx.source.id {
            weight = debt.weight;
            if debt.status == "open" { stage = "owed"; }
            if debt.status == "fulfilled" { stage = "honoured"; }
            if debt.status == "broken" { stage = "repudiated"; }
        }
    }

    let actions = [];
    let player_head = 0;
    if ctx.bindings.creditor == ctx.world.player_org {
        for organisation in ctx.world.organisations {
            if organisation.id == ctx.world.player_org {
                player_head = organisation.head;
            }
        }
        if player_head > 0 {
            actions.push(#{
                id: "call-favour", leader: player_head,
                target_kind: "organisation", target_a: ctx.bindings.debtor,
            });
        }
    }

    #{
        stage: stage,
        metrics: [#{ label_key: "situation.metric.weight", value: weight }],
        participants: [
            #{ kind: "organisation", id: ctx.bindings.debtor },
            #{ kind: "organisation", id: ctx.bindings.creditor },
        ],
        actions: actions,
        resolution_values: #{
            debtor: organisation_name(ctx.world, ctx.bindings.debtor),
            creditor: organisation_name(ctx.world, ctx.bindings.creditor),
        },
    }
}

fn favour_was_honoured(ctx) {
    for debt in ctx.world.obligations {
        if debt.id == ctx.source.id { return debt.status == "fulfilled"; }
    }
    false
}

fn favour_was_repudiated(ctx) {
    for debt in ctx.world.obligations {
        if debt.id == ctx.source.id { return debt.status == "broken"; }
    }
    false
}

define_situation(#{
    id: "formal-war",
    source: "scenario",
    bindings: #{ war: "war" },
    trigger_fn: "formal_war_instances",
    projection_fn: "formal_war_projection",
    priority: 90,
    log_activation: true,
    stages: ["open", "under-pressure", "ended"],
    actions: [
        #{ id: "adopt-attacker", assignment: "adopt-formal-war" },
        #{ id: "adopt-defender", assignment: "adopt-formal-war" },
        #{ id: "negotiate", assignment: "negotiate" },
        #{ id: "besiege", assignment: "besiege" },
        #{ id: "blockade", assignment: "blockade" },
    ],
    outcomes: [
        #{ id: "peace", when_fn: "war_ended_in_peace" },
        #{ id: "ended", fallback: true },
    ],
});

fn formal_war_instances(ctx) {
    let instances = [];
    for war in ctx.world.wars {
        if war.active {
            instances.push(#{ war: war.id });
        }
    }
    instances
}

fn formal_war_projection(ctx) {
    let stage = "ended";
    let warning = false;
    let participant_groups = [];
    let links = [];
    let actions = [];
    let values = #{};
    let player_head = 0;
    let player_armies = [];
    let player_ships = [];
    let current_operations = 0;
    let current_blockades = 0;

    for work in ctx.world.assignments {
        if type_of(work.war) == "i64" && work.war == ctx.bindings.war {
            current_operations += 1;
            links.push(#{ kind: "assignment", id: work.id });
        }
    }

    for organisation in ctx.world.organisations {
        if organisation.id == ctx.world.player_org {
            player_head = organisation.head;
        }
    }
    for army in ctx.world.armies {
        if army.owner == ctx.world.player_org {
            player_armies.push(army);
        }
    }
    for ship in ctx.world.ships {
        if type_of(ship.blockade_war) == "i64"
                && ship.blockade_war == ctx.bindings.war {
            current_blockades += 1;
            links.push(#{ kind: "ship", id: ship.id });
        }
        if ship.owner == ctx.world.player_org
                && type_of(ship.captain) == "i64" {
            player_ships.push(ship);
        }
    }

    for war in ctx.world.wars {
        if war.id == ctx.bindings.war {
            values["attacker"] = organisation_name(ctx.world, war.sides[0].leader);
            values["defender"] = organisation_name(ctx.world, war.sides[1].leader);
            let attackers = [];
            for member in war.sides[0].members {
                attackers.push(#{ kind: "organisation", id: member });
            }
            let defenders = [];
            for member in war.sides[1].members {
                defenders.push(#{ kind: "organisation", id: member });
            }
            participant_groups.push(#{
                label_key: "situation.formal-war.side.attackers",
                participants: attackers,
            });
            participant_groups.push(#{
                label_key: "situation.formal-war.side.defenders",
                participants: defenders,
            });

            let player_side = -1;
            let enemy_provinces = [];
            if war.active {
                stage = "open";
                if war.sides[0].members.contains(ctx.world.player_org) { player_side = 0; }
                if war.sides[1].members.contains(ctx.world.player_org) { player_side = 1; }
                if player_side >= 0 {
                    let enemies = war.sides[1 - player_side].members;
                    for province in ctx.world.provinces {
                        if type_of(province.holder) == "i64"
                                && enemies.contains(province.holder) {
                            enemy_provinces.push(province.id);
                        }
                    }
                    for army in ctx.world.armies {
                        if enemies.contains(army.owner) {
                            for province in ctx.world.provinces {
                                if province.holder == ctx.world.player_org && province.id == army.location {
                                    warning = true;
                                    stage = "under-pressure";
                                }
                            }
                        }
                    }
                }
            }

            if war.active {
                let adopt_attacker = #{
                    id: "adopt-attacker",
                    target_kind: "war-side", target_a: war.id, target_side: "attacker",
                };
                let adopt_defender = #{
                    id: "adopt-defender",
                    target_kind: "war-side", target_a: war.id, target_side: "defender",
                };
                let negotiate = #{
                    id: "negotiate",
                    target_kind: "war", target_a: war.id,
                };
                if player_head > 0 {
                    adopt_attacker["leader"] = player_head;
                    adopt_defender["leader"] = player_head;
                    negotiate["leader"] = player_head;
                }
                actions.push(adopt_attacker);
                actions.push(adopt_defender);
                actions.push(negotiate);

                for army in player_armies {
                    for province in enemy_provinces {
                        actions.push(#{
                            id: "besiege", leader: army.general,
                            target_kind: "army-to-province",
                            target_a: army.id, target_b: province,
                            context: [
                                #{ kind: "army", id: army.id },
                                #{ kind: "province", id: province },
                            ],
                        });
                    }
                }
                for ship in player_ships {
                    for province in enemy_provinces {
                        actions.push(#{
                            id: "blockade", leader: ship.captain,
                            target_kind: "ship-to-province",
                            target_a: ship.id, target_b: province,
                            context: [
                                #{ kind: "ship", id: ship.id },
                                #{ kind: "province", id: province },
                            ],
                        });
                    }
                }
            }

            return #{
                stage: stage,
                warning: warning,
                metrics: [
                    #{ label_key: "situation.metric.attacker-members", value: war.sides[0].members.len },
                    #{ label_key: "situation.metric.defender-members", value: war.sides[1].members.len },
                    #{ label_key: "situation.metric.operations", value: current_operations },
                    #{ label_key: "situation.metric.blockades", value: current_blockades },
                ],
                participant_groups: participant_groups,
                links: links,
                actions: actions,
                resolution_values: values,
            };
        }
    }

    #{ stage: stage, resolution_values: values }
}

fn war_ended_in_peace(ctx) {
    for war in ctx.world.wars {
        if war.id == ctx.bindings.war {
            return !war.active && war.conclusion_kind == "negotiated-peace";
        }
    }
    false
}

fn organisation_name(world, id) {
    for organisation in world.organisations {
        if organisation.id == id { return organisation.name; }
    }
    "Unknown organisation"
}

fn character_name(world, id) {
    for character in world.characters {
        if character.id == id { return character.name; }
    }
    "Unknown character"
}
