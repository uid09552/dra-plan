---
title: Standards Mapping (NIST SP 800-34, BSI 200-4)
type: domain
status: draft
tags: [standards, nist, bsi, compliance]
created: 2026-09-27
updated: 2026-09-27
related: [plan-authoring, dr-plan-model, glossary, 0005-standards-basis]
---

# Standards Mapping

The workflow ([[plan-authoring]]) follows two reference frameworks:

- **NIST SP 800-34 Rev. 1:** *Contingency Planning Guide for Federal Information Systems*. It covers IT system contingency planning, including the ISCP (Information System Contingency Plan) and the DRP (Disaster Recovery Plan).
- **BSI-Standard 200-4:** *Business Continuity Management* (it replaces BSI-Standard 100-4). It is supported by the IT-Grundschutz modules **DER.4 Notfallmanagement** and **CON.3 Datensicherungskonzept**.

Supplementary references:
- **NIST SP 800-184** (*Guide for Cybersecurity Event Recovery*) for ransomware and security scenarios.
- **NIST CSF 2.0**, *Recover (RC)* function.
- **ISO 22301** / **ISO/IEC 27031**.

> The content below summarizes these standards for product design. Check details against the current official publications before claiming compliance.

## NIST SP 800-34: the seven-step contingency planning process

| # | NIST step | Where it lives in dra-workflow |
|---|-----------|------------------------------|
| 1 | Develop the contingency planning policy | Tenant settings: roles, review interval, impact categories |
| 2 | Conduct the business impact analysis (BIA): identify processes and recovery criticality (MTD, RTO, RPO), resource requirements, and recovery priorities | Steps 1–3, 7 |
| 3 | Identify preventive controls | Step 2 (dependencies), step 8 (`data_protection`) |
| 4 | Create contingency strategies (backup and recovery, alternate sites, equipment replacement) | Step 8 |
| 5 | Develop the contingency plan | Steps 9–12 |
| 6 | Ensure plan testing, training and exercises (TT&E) | Steps 13–14 |
| 7 | Ensure plan maintenance | Step 15, `next_review_due` |

**NIST plan phases** are used for `runbook_step.phase`: **Activation & Notification → Recovery → Reconstitution**. They are preceded by supporting information (concept of operations, roles) and followed by appendices (contacts, vendor contacts, procedures, validation test plan, diagrams, inventory).

**NIST impact level** (FIPS 199 availability: low, moderate or high) is stored as `it_service.impact_level`.

## BSI-Standard 200-4: Business Continuity Management

BSI 200-4 describes a staged approach (**Reaktiv-BCMS → Aufbau-BCMS → Standard-BCMS**). The core BCM process:

| BSI element | Meaning | Where it lives in dra-workflow |
|-------------|---------|-------------------------------|
| Initiierung, Leitlinie | Start BCM and define its scope and policy | Tenant settings |
| BAO (*Besondere Aufbauorganisation*) | The special organizational structure for emergencies and crises (crisis staff and roles) | `role`, `role_assignment`, step 10 |
| BIA: Vorfilterung, Schadensszenarien, Prozessbewertung | Pre-filter, damage categories, assessment of impact over time | Step 3, `impact_rating` |
| MTA (*maximal tolerierbare Ausfallzeit*) | MTPD | `business_impact_analysis.mtpd_minutes` |
| WAZ (*Wiederanlaufzeit*) | RTO | `service_rto_minutes`, `recovery_objective.rto_minutes` |
| MTDV (*maximal tolerierbarer Datenverlust*) | RPO | `service_rpo_minutes`, `recovery_objective.rpo_minutes` |
| Notbetriebsniveau | Minimum operating level (MBCO) | `minimum_operating_level` |
| Ressourcen / Abhängigkeiten | Resources the process needs | Step 2, `dependency` |
| Risikoanalyse (BC) | Threats to critical resources | Steps 4–6, `scenario` |
| Soll-Ist-Vergleich (IT-SCM) | Required recovery capability compared with actual capability | Step 8 gap check |
| Kontinuitätsstrategien | Continuity strategies | Step 8, `recovery_strategy` |
| Notfallhandbuch, Geschäftsfortführungspläne, Wiederanlauf- / Wiederherstellungspläne | Emergency handbook, business continuity plans, restart and restoration plans | Steps 9–12, export of `dr_plan_version` |
| Alarmierung / Eskalation | Alerting and escalation | Step 11, `communication_rule` |
| Tests und Übungen | Tests and exercises: plan review, tabletop and staff exercises, alerting drills, functional/recovery tests, full exercises | Steps 13–14, `dr_test` |
| Kontinuierliche Verbesserung | Continuous improvement | Step 15, `action_item` |

**BSI protection requirement for availability** (*Schutzbedarf Verfügbarkeit*: normal, high or very high) is stored as `it_service.protection_requirement_availability`.

BSI distinguishes these operating states, which map to NIST phases:

| BSI state | NIST phase |
|-----------|-----------|
| Notfall / Alarmierung | Activation & Notification |
| Notbetrieb | Recovery (running at the minimum operating level) |
| Wiederanlauf | Recovery |
| Wiederherstellung | Reconstitution |
| Normalbetrieb | Reconstitution complete |

## Combined phase view

| dra-workflow phase | NIST SP 800-34 | BSI 200-4 |
|--------------------|----------------|-----------|
| A · Understand | Step 2 (process and resource identification) | Scope, process and resource inventory |
| B · Assess | Step 2 (BIA) | BIA and risk analysis |
| C · Design | Steps 3–4 | Soll-Ist-Vergleich, continuity strategies |
| D · Document | Step 5 | Emergency handbook, restart and restoration plans, BAO |
| E · Validate & Maintain | Steps 6–7 | Tests and exercises, continuous improvement |
| Recovery execution | Activation → Recovery → Reconstitution | Alarmierung → Notbetrieb/Wiederanlauf → Wiederherstellung |
