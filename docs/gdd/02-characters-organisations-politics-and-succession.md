# Last Aeon — Characters, Organisations, Politics, and Succession

| Field | Value |
| --- | --- |
| Document status | Draft 0.1 |
| Scope | GDD follow-up 2 of 8 |
| Scenario focus | The Ashkarr Succession |
| Primary design authority | `pasm/spec/` |
| Runtime authority | `crates/aeon_sim/` |
| Setting authority | `the_last_aeons/` |

**GDD navigation:** [Index](README.md) · [Overview](overview.md) · [Previous: Player Experience](01-player-experience-campaign-and-onboarding.md) · [Next: Assignments, Forecasts, Plans, and Goals](03-assignments-forecasts-plans-and-goals.md)

This document expands the character and political systems introduced in the
[Game Design Overview](overview.md). It describes the current playable rules,
the accepted design contract, and questions that still need a design decision.
It does not replace PASM or promote a proposal into an accepted rule.

## Status language

Every substantive statement in this document uses one of three statuses:

- **Implemented/current** means the authoritative simulation or shipped
  content performs the behaviour now, with tests or client presentation where
  cited.
- **Accepted design** means PASM records the behaviour as accepted, even if the
  complete player-facing implementation is not yet evident in the runtime.
- **Proposal** means a candidate clarification for later review. It is not a
  commitment and must not override an unmarked PASM decision.

Where PASM and the current runtime differ, both are stated explicitly.

## Purpose

Characters and organisations turn the campaign from territorial optimisation
into personal rule. The organisation is the durable strategic identity; its
current leader and members are the finite people through whom it can act. Legal
power is expressed through titles, vassalage, obligations, and revocable
offices. Opinion describes how one person feels about another, while specific
political facts explain what people and houses are entitled, expected, or
compelled to do.

Succession is the system that makes this distinction consequential. A ruler's
death should change the player’s abilities, relationships, personal claims,
appointments, and immediate problems without routinely erasing the house and
its accumulated position.

## Player experience

The player should be able to answer five political questions without reverse
engineering the simulation:

1. **Who am I ruling through?** The current head, their family, skills, traits,
   location, health, work, titles, offices, and personal political standing.
2. **Who belongs to my organisation?** The living household and court available
   to lead assignments, inherit, command, marry, or hold office.
3. **Who holds authority over whom?** Direct title holders, lieges, vassals,
   independent houses, and the distinct Sanctora chain of authority.
4. **What binds us?** Directional opinion, family facts, declared claims,
   favours, promises, grievances, wars, and other named political records.
5. **What changes when someone dies?** The heir, vacated personal titles and
   offices, lost claims, leaderless forces, and any risk to campaign continuity.

The intended emotional rhythm is continuity under disruption: a succession
should preserve the house while making yesterday's secure position feel newly
personal, conditional, and contestable.

## Political model at a glance

| Concept | Attaches to | Lifetime | Current Ashkarr use |
| --- | --- | --- | --- |
| Character | A specific person | Birth to death; remains in history | Leader, family member, delegate, captain, office-holder, claimant |
| Organisation | A house or rules-distinct institution | Persists across leaders unless defunct | Player identity, resources, members, liege, holdings, armies, ships |
| Opinion | One character toward another | Derived now, plus expiring or persistent modifiers | Relations display and political weighting |
| Family fact | Specific characters | Until changed by birth, marriage, or death | Parentage, sibling derivation, spouse, succession order |
| Territorial title | Province, body, or other legal holding | Enduring legal object; holder can change | Province ownership and the Paramountcy |
| Office | Delegated authority under an organisation | Temporary and revocable | Starbase Commander |
| Personal title | A character | Until vacated by death or another rule | Consul and Paramount of Ashkarr |
| Vassalage | A subordinate organisation to its direct liege | Until political change | Eight named vassal houses under three Great Houses |
| Political obligation | Debtor organisation to creditor organisation | Open until fulfilled, broken, or expired | Favours, promises, and grievances |
| Paramount claim | A character to a vacant Paramount title | While its claimant remains eligible | Declared route into the Ashkarr succession crisis |

## Characters

### Implemented/current

- Each simulated character has a stable ID, display name, sex, birth date,
  optional death date, optional organisation, four base skills, authored
  traits, up to two parents, an optional spouse, and a directional opinion
  ledger.
- Authored characters and characters born during play share the same runtime
  model. Runtime-born characters have no authored content key.
- Characters age, face deterministic yearly mortality, can marry, and can have
  children. Death remains an inspectable state rather than deleting the person.
- The broader population is abstracted. The individual simulation covers the
  player household, courts, rulers, rivals, and other strategically relevant
  figures.
- A character can lead assignments only through the authoritative eligibility
  and command path described in the assignments GDD. Their organisation,
  presence, current work, and other state constrain availability.
- In the authored House Harrow start, Edrun Harrow is the head. Kessarin is his
  spouse; Senna and Brant are their children; Aleyn is his sibling; Reyn is
  Aleyn's spouse; Mikael, Torvald, and Captain Lira provide additional household
  and court capacity.

### Accepted design

- A house never acts impersonally: a character wields its authority. The
  organisation's pressures may shape a decision, but the acting person's
  capabilities and circumstances determine what can be considered and done.
- The current leader is exposed to incapacity, personal risk, death, opinion,
  and succession. Organisation authority must not become an abstract state
  controller detached from that leader.
- Characters are the acting unit for both player and autonomous organisations;
  AI organisations do not receive a separate, privileged action model.

### Proposals requiring approval

- Define a small player-facing **political role vocabulary**—head, heir,
  household member, courtier, captain, office-holder, claimant—derived from
  existing facts rather than stored as a generic relationship type.
- Add an inspectable succession preview naming the current expected heir and the
  rule that placed them first, while warning that births and deaths may alter
  the result.
- Decide whether health and incapacity need richer visible states before the
  accepted promise of incapacity can be considered playable.

## Organisations and leadership

### Implemented/current

An organisation record stores its stable identity, authored kind, optional
house tier, direct liege, current head, and whether it is defunct. Resources,
forces, obligations, assignments, plans, goals, and holdings connect to that
stable organisation identity rather than being owned by the current head.

The Ashkarr field contains:

- three Great Houses: Veyrin, Draksha, and Meloch;
- eight vassal houses, including player-led House Harrow;
- two independent houses, Pell and Szel; and
- the Sanctora Imperim, a non-dynastic organisation controlling the Aurelian
  Spire and initially led by Consul Aurelia Veth.

House Harrow begins as a direct Veyrin vassal with four provinces. Its
organisation survives an ordinary change of head, retaining its holdings,
resources, liege, forces, and organisation-level obligations.

The simulation answers two different hierarchy questions:

- **Who is this house's top-level Great House?** Follow vassal links upward.
- **Is this holding mine or subordinate to me?** Measure the number of direct
  liege hops from holder to the organisation being inspected. A vassal's own
  land remains its own; it does not become the direct property of its liege.

Both traversals are bounded to prevent malformed live hierarchy data from
hanging the campaign. Content validation remains responsible for rejecting
cycles.

### Accepted design

- The enduring player identity is an organisation; for the current slice it is
  the fixed dynastic House Harrow.
- Dynastic houses combine family, headship, titles, assets, characters,
  obligations, and hereditary succession.
- The Sanctora Imperim follows different political and leadership rules. Its
  Consul is a Tsar-appointed authority rather than a hereditary house head.
- Finished-game organisation types may use the same succession framework with
  distinct rules, but the MVP must not generalise prematurely beyond playable
  dynastic succession.

The worldspec supports the Sanctora distinction: it describes the Sanctora
Imperim as the Golden Tsar's civilian government, with each sector governed by
a Tsar-appointed Consul and backed by the Sanctora Guard. Ashkarr-specific
houses and succession details come from the game scenario and PASM, not from
the wider worldspec.

## Relationships and opinion

### Implemented/current

Opinion is directional: A's opinion of B can differ from B's opinion of A. It
is calculated at the point of use and clamped from -100 to +100.

The current calculation combines:

- affinity with the target's same traits and penalties for authored opposite
  traits;
- same-organisation membership;
- spouse, parent-child, and sibling bonds; and
- stored, target-specific modifiers, each identified by a stable reason and
  optionally expiring on a date.

Self-opinion is +100. Expired modifiers stop contributing and are cleaned up.
The client exposes mutual opinion in character inspection and visualises other
heads' opinions of the player head in the relations map mode.

[ai] Household demands are now a stored-modifier origin. When a First
Reign demand resolves, the bound requester gains one directional modifier
toward the house head under the tier's own stable reason: achievement
+10 for 1,440 days, an explicit refusal left to stand -5 for 1,080,
silence -10 for 1,440, and a broken promise -20 for 1,800. One reason per
tier means tiers can never stack on a single lifecycle, and a repeat of
the same tier refreshes rather than accumulates — consistent with the
one-modifier-per-(target, reason) rule above. Reasons are also
per-definition (the three demand definitions carry twelve reasons among
them — Kessarin's Order, Aleyn's Levies, and Torvald's Standing each
their own four), so when demands pass to the same successor, each
demand's tier lands under its own reason and they never collapse into
one modifier. The magnitudes and
durations are authored scenario content, not Rust constants, and the
modifiers are sentiment only: the demand's promise/refusal record lives
on the Situation, not in the obligation ledger, respecting the accepted
rule that opinion must not substitute for separate political facts.
Derived net opinion is also readable as a Situation goal: Torvald's
Standing judges the bound liege head's live derived opinion of the
house head against an authored target, consuming the same authoritative
opinion facts the inspectors expose rather than any private counter, so
any legitimate relationship effect — courting, insult, whispers, or a
third party's doing — moves the goal in either direction.

### Accepted design

There is deliberately no generic relationship entity. Marriage, lineage,
title ownership, liege links, office, obligations, claims, secrets, factions,
and wars are separate facts because each has different ownership, rules,
lifetimes, and consequences. Opinion is sentiment; it must not substitute for
legal or political facts.

Likewise, favours, promises, and grievances remain individually inspectable
organisation-level records even where the UI or AI also presents a net
standing. A disliked house may still owe a favour, and a liked house may still
hold a grievance.

### Proposals requiring approval

- Show an opinion breakdown rather than only the final score: trait affinity,
  kinship, shared organisation, and each named modifier.
- Explicitly label the direction of every score—“A's opinion of B”—to prevent a
  symmetric-relationship reading.
- Decide whether head-to-head opinion is sufficient for all organisation-level
  political presentation or whether some actions need a separately accepted
  organisational reputation model.

## Titles, offices, holdings, and vassalage

### Titles and offices are not interchangeable

| Territorial or personal title | Office |
| --- | --- |
| Enduring legal object over a province, body, or recognised dignity | Temporary grant of authority and responsibility |
| May be held by an organisation, held personally, or be vacant | Held by a character or vacant |
| Can be inherited, claimed, granted, seized, or vacated according to its own rule | Can be revoked or refilled by the appointing authority |
| Current examples: province titles, Paramountcy, Consulate | Current example: Starbase Commander |

This separation is accepted PASM design. It allows a landed house to possess
durable power while an appointee temporarily exercises institutional authority.

### Implemented/current

- Every province has a legal title. The current holder variants are an
  organisation, a character, or vacant.
- Province titles in Ashkarr are held by organisations. Successful conquest can
  transfer a province title to the attacking organisation.
- A title can cover a province, a body's Paramountcy, or the Consulate.
- The Starbase Commander is an office of the Sanctora Imperim tied to the
  Spire's province. Commander Dray holds it at campaign start.
- Office records identify the organisation whose authority they carry, their
  optional administered province, holder, and vacancy date.
- The Consul automatically fills a Sanctora office after its vacancy timer with
  the living organisation member who has the best combined command and
  stewardship score; stable ID breaks ties.
- Vassalage is represented by each subordinate house naming one direct liege.
  Current hierarchy helpers expose direct vassals, top-level realm, and bounded
  hop distance.

### Accepted design and current limit

PASM accepts a broader legal-holdings model in which holdings may be personally
held by a house, administered for a superior, leased, or controlled through a
recognised arrangement. It also expects vassalage, contracts, and appointments
to make taxes, levies, claims, loyalty, rebellion, and succession legible.

The current runtime evidence is narrower: it represents direct title holder,
organisation liege, and office authority, but does not expose distinct title
tenure variants for administration, lease, or contract. Those broader modes
should therefore be treated as accepted direction, not as current player
rules.

### Proposals requiring approval

- Before adding tenure modes, define which facts belong on a title, an office,
  an organisation-to-organisation contract, or the obligation ledger.
- Present both **direct holder** and **ultimate realm** wherever a vassal-held
  province appears; neither should silently replace the other.
- Establish explicit invalidation rules for liege changes: active wars, plans,
  directives, obligations, Paramount claims, and assignment eligibility must
  all revalidate against the new hierarchy.

## Obligations and political continuity

### Implemented/current

An obligation is a bilateral organisation fact, separate from character
opinion. Its detailed schema, settlement, expiry, derived standing, Situation
integration, and presentation are owned by
[Economy, Order, Obligations, Situations, Intrigue, and Warfare](05-economy-order-obligations-situations-intrigue-and-warfare.md#political-obligations).

House Harrow begins owing Veyrin a favour for support during a dearth. Other
authored obligations connect lieges, vassals, independents, and the Sanctora,
giving the crisis a political history before the first command is issued.

Because these records attach to organisations, an ordinary succession does not
erase them. Personal opinion and personal claims may change while the house's
debts and grievances remain.

## Succession and appointments

### Dynastic succession: implemented/current

When a dynastic house head dies, the simulation selects the first living member
of that organisation in this gender-blind order:

1. children of the dead head, oldest first;
2. siblings of the dead head, oldest first; then
3. any other living organisation member, oldest first.

Stable character ID resolves any remaining tie. The implementation does not
require the heir to be an adult, popular, nearby, capable, or uncompromised.
The selected heir immediately becomes organisation head.

Death also performs related political cleanup:

- the dead character's personal Paramount declarations are removed before
  succession, so the heir cannot inherit them implicitly;
- a surviving spouse is widowed;
- ships captained by the dead character promote an eligible aboard First Officer
  seamlessly; only a ship without one becomes leaderless and begins its
  automatic refuge behaviour;
- personally held titles become vacant;
- held offices become vacant and begin their appointment timer; and
- Consular contest eligibility is re-evaluated after new heads exist.

If no living member exists, the house becomes defunct and leaderless. The
accepted terminal check observes this after the day has fully settled.

### Dynastic succession: accepted terminal rule

The campaign continues as any living character selected by the organisation's
legal succession rules. Age, popularity, health, location, captivity, and
incapacity may constrain the resulting reign but do not invalidate that
successor.

Two independent conditions can end the dynastic MVP campaign: no legal living
successor, **or** no province title directly held by the player organisation.
Vassal-held provinces and personal titles, armies, or ships are not a meaningful
territorial foothold. The check runs after the day and all boundary pulses have
fully settled; the first settled day satisfying either condition ends play,
without a grace period. The runtime implements both branches at that settled-day
boundary.

PASM also mentions a head who “dies or is removed.” The inspected current
succession path is death-driven. Non-death removal or replacement should not be
presented as implemented until an authoritative command or system exercises it.

### Consular succession: implemented/current

The Consulate is a personally held title and the practical leadership of the
Sanctora Imperim. When vacant, it opens a fixed-duration contest among the
living adult heads of organisations and adult Sanctora members who were
eligible when the slate opened. Candidate score combines twice diplomacy,
stewardship, the summed opinion of other living Sanctora members, and a
candidate-specific deterministic variation; the highest score wins, with lower
stable ID breaking a tie. The winner personally receives the Consul title and
becomes head of the Sanctora Imperim.

If every candidate dies while the title remains vacant, the contest restarts
with the current eligible slate and a fresh full appointment period. The title
cannot remain silently stranded because its original candidates died.

This implements the local political result of a formal appointment by the
distant Golden Tsar. It is not hereditary succession.

## The Ashkarr Paramountcy and personal claims

### Implemented/current

The Paramountcy of Ashkarr begins vacant after the previous Paramount died
without an accepted successor. Any living adult head of a non-defunct,
independent dynastic house may declare a personal claim while the title remains
vacant. The claim may be tenuous; declaration does not require present
dominance.

A claim remains valid only while the same person:

- is alive and adult;
- remains their dynastic house's current head;
- leads a non-defunct organisation;
- remains independent, with no liege; and
- contests a still-vacant Paramountcy.

Claims can be renounced. Invalid claims are removed by cleanup, and death
removes them before succession. A new head must declare anew.

To press the claim successfully, the claimant must still be eligible, must have
declared personally, and their complete top-level realm must hold strictly more
provinces on Ashkarr than every rival realm. A tie is not dominance. Active war
against another currently valid claimant blocks the award until resolved. On
success, the Paramount title is held personally and all declarations to that
title are cleared.

Realm pressure includes a claimant's transitive vassal branch, while direct
holding display remains distinct. The current client exposes this competition
through the claim-pressure map mode.

### Accepted design

- Paramount claims are explicit choices by characters, not scores silently
  attached to houses.
- Independence makes the claim an assertion of sovereign authority.
- The successful Paramount holds the title personally; death vacates it rather
  than passing it to an heir.
- Consular support can help or oppose a claimant but is neither necessary nor
  sufficient by itself to award the title.

### Proposals requiring approval

- Show declaration eligibility, current dominance totals, tied rivals, and any
  claimant war blocker together on the Paramountcy Situation.
- On succession, create a prominent but non-blocking notice that the former
  head's claim ended and the new head must declare anew if eligible.
- Decide what “Consular endorsement” changes mechanically; the accepted design
  establishes its importance and limits, but this document does not infer a
  specific modifier or assignment.

## Player-facing rules and information contract

The following rules should be communicated wherever a player can act on them:

- Name the **person** taking an action and the **organisation** whose authority
  they carry.
- Distinguish direct title holder, personal title holder, office-holder, direct
  liege, and ultimate realm.
- Display opinion direction and its current value; do not present obligations
  as opinion modifiers.
- State whether a political fact survives succession. Organisation resources,
  holdings, liege, forces, and obligations normally do; personal claims,
  opinions, titles, offices, and capabilities may not.
- Show the expected heir and succession order before death, while avoiding a
  guarantee when the world can still change.
- Explain every blocked Paramount action using the authoritative eligibility
  predicate: vacancy, life and adulthood, dynastic headship, independence,
  declaration, strict realm dominance, and claimant war.
- Preserve political results in the log, Situation history, inspector, or
  ledger rather than relying only on transient notifications.

## Data and authoring

### Implemented/current data

| Record | Principal authored or runtime fields |
| --- | --- |
| Character | Stable ID, optional content key, name, sex, birth/death, organisation, skills, traits, parents, spouse, opinion modifiers |
| Organisation | Stable ID, content key, kind, house tier, liege, head, defunct state, colour and starting resources through content |
| Title | Stable ID, optional content key, name, kind, holder; province titles are also indexed by province |
| Office | Stable ID, key, name, organisation, optional province, holder, vacancy date |
| Obligation | Ledger ID, optional authored source, kind, debtor, creditor, origin, dates, weight, status |
| Paramount claim | Title, claimant character, declaration date |
| Consular contest | Title, opened date, frozen candidate list; score is derived from skills, Sanctora opinion, and deterministic variation |

All durable references use stable game IDs rather than transient ECS entities.
Politics, obligations, personal claims, and appointment contests are captured
in campaign snapshots and restored against the same validated content. Ordered
collections and stable tie-breaks preserve deterministic replay.

### Authoring rules inferred from current validation and runtime

- Character family and organisation references must resolve.
- An authored head must belong to the organisation they lead.
- A vassal names one valid liege and hierarchy data must not cycle.
- Title and office holders must be valid characters or organisations of the
  appropriate record shape.
- Political obligation parties must be different valid organisations.
- Authored opinions and obligations use named reasons or origins so the result
  can be inspected rather than appearing as an unexplained number.

## Edge cases and required behaviour

| Edge case | Current or required handling |
| --- | --- |
| Head dies with an underage child | Current: the oldest living child can inherit; no adulthood gate |
| Head dies with no child but a sibling | Current: oldest living sibling in the organisation inherits |
| Head dies with only an unrelated house member | Current: oldest living member inherits |
| Player has no legal living successor after the day settles | Required: campaign ends even if the house still owns provinces |
| Player ends a settled day with no directly held province | Required: campaign ends even if a legal heir, vassal holdings, armies, ships, or personal titles remain |
| Spouse dies | Current: surviving spouse link is cleared |
| Captain or General dies | Required: eligible First Officer or Lieutenant promotes immediately and preserves orders; otherwise the leaderless force accepts no ordinary orders and retreats at half speed |
| Personal title-holder dies | Current: title becomes vacant; Consul vacancy opens a contest |
| Office-holder dies | Current: office becomes vacant and enters its appointment timer |
| Paramount claimant dies | Current: claim is removed before heir selection; heir does not inherit it |
| Claimant accepts a liege | Current: claim fails eligibility and is cleaned up |
| Paramount title is no longer vacant | Current: all outstanding claims cease to be valid; successful press clears them |
| Two realms tie for most provinces | Current: neither is dominant; neither can press on dominance alone |
| Claimant leads but fights another valid claimant | Current: the active claimant war blocks pressing |
| Every Consular candidate dies | Current: the contest restarts with a fresh eligible slate and timer |
| Opinion modifier expires | Current: it stops counting at expiry and is removed by cleanup |
| Political hierarchy is malformed at runtime | Current: traversals stop after a bounded depth; content validation should reject the source |
| Organisation has no head | Accepted/current agency rule: it cannot act autonomously until authority is restored |

## Feedback and presentation

### Implemented/current

- Organisation inspection shows status, direct liege, titles held, living
  members, directives where relevant, and open obligations with origin, weight,
  and expiry.
- Character inspection shows organisation, family facts, skills, traits,
  assignments or plans, and directional opinion relative to the player head.
- Map modes distinguish direct holder, greater-realm allegiance, relations, and
  Paramount claim pressure.
- Political and military log entries record major changes such as a successful
  Paramount claim or a ship losing its captain.
- Situations can use characters, organisations, titles, offices, and obligations
  as inspectable subjects and action targets.

### Presentation still to specify

- succession preview and post-succession comparison;
- a complete title and office history;
- opinion component breakdown;
- claim eligibility and dominance breakdown in one place;
- visibility rules for foreign family, modifier, obligation, and claim details;
- treatment of dead characters in genealogy and history views; and
- campaign-end feedback that reflects the eventually accepted viability and
  foothold rule.

## Dependencies

This system depends on:

- the calendar and daily/yearly pulses for age, mortality, expiry, appointments,
  marriage, birth, and cleanup;
- stable IDs, deterministic derived RNG, ordered iteration, snapshots, command
  logging, and state hashing;
- validated scenario, trait, organisation, character, title, office, and
  obligation definitions;
- map and province titles for holdings, realm dominance, and territorial
  continuity;
- presence, assignments, plans, and goals for who can act and where;
- resources, legitimacy, influence, obligations, and provincial order for
  political leverage;
- warfare for title transfer, rebellion, claimant conflict, and realm branches;
  and
- client inspectors, map modes, Situations, ledgers, and logs for legibility.

## Acceptance criteria

### Baseline: current and accepted behaviour

- The same seed and ordered commands produce the same births, deaths,
  marriages, heirs, appointments, opinions, claims, and political state hash.
- Snapshot restore preserves characters, family links, organisation heads,
  lieges, titles, offices, obligations, personal claims, and open contests.
- A dynastic head's death selects the legal heir in the current documented
  order and does not transfer personal Paramount claims.
- The player campaign continues through an ordinary succession.
- The Consulate and Paramountcy follow their distinct, documented vacancy and
  appointment or claim rules.
- Direct ownership, vassal hierarchy, and complete-realm calculations remain
  distinct and are tested for a vassal player's own holdings.
- Opinion remains directional, bounded, derived from visible categories, and
  separate from specific political obligations.
- Invalid political content is rejected rather than repaired nondeterministically
  during play.
- Player-visible eligibility and outcome explanations are produced from the
  same authoritative predicates used for resolution.

### Before this GDD can become final

- Verify campaign-end feedback for both accepted settled-day failure reasons in
  the final interface treatment.
- Specify or narrow the accepted promise of non-death head removal and
  incapacity.
- Define which broader holding arrangements—administration, lease, contract,
  or appointment—belong in the first release and their distinct data rules.
- Specify information visibility for foreign political facts.
- Decide the mechanical role of Consular endorsement in the Paramount crisis.
- Approve the succession, title, office, opinion, and claim feedback contract
  before treating the proposed UI rules as requirements.

## Evidence and traceability

| Source | Evidence used |
| --- | --- |
| `pasm/spec/core/game-vision.yaml` | Player organisation and head; dynastic houses; Sanctora distinction; character scope; opinion; no generic relationship; legal holdings; titles versus offices; succession framework; Paramount and Consular rules; obligations; characters as acting units |
| `pasm/spec/roadmap/milestone-5-persons-and-plans.yaml` | Character-led autonomous action and plan presentation |
| `pasm/spec/roadmap/milestone-8-grand-strategy.yaml` | Vassal branches, directives, rebellion, formal war, and claim-pressure context |
| `pasm/spec/roadmap/milestone-10-shadows.yaml` | Character-targeted harm, persistent grievance, and personal risk context |
| `assets/content/scenario/ashkarr-succession.rhai` | Fixed player house, political hierarchy, Harrow family and court, traits, titles, office, vacant Paramountcy, Consul, and starting obligations |
| `crates/aeon_sim/src/politics.rs` | Runtime character, lineage, opinion, organisation, title, office, succession, life-cycle, and appointment rules |
| `crates/aeon_sim/src/crisis.rs` | Personal Paramount claim eligibility, cleanup, realm dominance, claimant-war blocker, and award |
| `crates/aeon_sim/src/obligations.rs` | Bilateral obligation data, standing, settlement, history, and expiry |
| `crates/aeon_sim/src/wars.rs` and `warfare.rs` | Vassal branches, formal war authority, and province title transfer |
| `crates/aeon_sim/tests/politics.rs` | Deterministic spawn and life simulation, opinion, succession, campaign failure, Consular contests, vassal-hop semantics, snapshots |
| `crates/aeon_sim/tests/crisis_wars.rs` and `scenario.rs` | Personal claim non-inheritance, independence, dominance, claimant war, authored field, and deterministic long run |
| `crates/aeon_client/src/ui/inspector.rs`, `map_modes.rs`, and `ui/lookup.rs` | Current player-facing organisations, obligations, mutual opinion, hierarchy, direct holding, realm, and claim-pressure presentation |
| `the_last_aeons/entities/import_09.md` | Canon basis for the Sanctora Imperim, sectoral Consuls, Golden Tsar appointment, and Sanctora Guard |

## Open questions

1. Can a player designate, influence, contest, or disinherit an heir, or is the
   current deterministic legal order intentionally absolute for the slice?
2. What gameplay consequences distinguish an underage, distant, unpopular, or
   incapacitated ruler beyond skill and presence constraints?
3. Which titles should be organisation-held, which personally held, and which
   hereditary in the first full release?
4. Who may revoke an office, on what grounds, and with what political cost?
5. Which tenure and contract forms are needed beyond direct holding, liege, and
   office before the broader accepted holdings model is considered delivered?
6. How much of another house's succession order, opinion breakdown, obligations,
   and claim position is public, inferred, concealed, or discovered?
7. What precise benefit can Consular endorsement grant without becoming either
   necessary or sufficient for the Paramountcy?
8. Can obligations transfer between organisations, be inherited personally, or
   bind successors differently, or are they always organisation-persistent?
9. How should simultaneous death, title transfer, war collapse, succession,
    claim cleanup, and appointment be ordered and explained when several occur
    on the same simulation day?

## Related GDD sections

- [Player experience, campaign, and onboarding](01-player-experience-campaign-and-onboarding.md)
- [Assignments, forecasts, plans, and goals](03-assignments-forecasts-plans-and-goals.md)
- [Economy, order, obligations, Situations, intrigue, and warfare](05-economy-order-obligations-situations-intrigue-and-warfare.md)
