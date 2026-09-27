---
title: Glossary
type: domain
status: active
tags: [glossary, domain]
created: 2026-09-27
updated: 2026-09-27
related: [dr-plan-model]
---

# Glossary

| Term | Meaning |
|------|---------|
| **DR** | Disaster Recovery: restoring IT services after a disruptive event. |
| **DRP** | Disaster Recovery Plan: a documented procedure for recovering a specific IT service. |
| **BCM / BCP** | Business Continuity Management / Plan: the broader organizational continuity process that DR is part of. |
| **BIA** | Business Impact Analysis: assesses the impact of a service outage over time and drives RTO/RPO. |
| **RTO** | Recovery Time Objective: the maximum acceptable time to restore a service. |
| **RPO** | Recovery Point Objective: the maximum acceptable data loss, measured in time. |
| **MTPD** | Maximum Tolerable Period of Disruption: the point where the impact becomes unacceptable. |
| **Criticality tier** | A classification of a service's importance (for example Tier 0–3) that drives RTO/RPO targets. |
| **Dependency** | Another service, infrastructure component or supplier that the service needs to operate. |
| **Runbook** | The ordered, executable recovery steps derived from a DRP. |
| **Recovery run** | One execution of a runbook, during a real disaster or a test. |
| **Failover / Failback** | Switching to the secondary site/system, and switching back to the primary. |
| **DR test** | A planned exercise (plan review, tabletop, simulation, technical recovery, full DR) that validates a DRP. |
| **MTTR** | Mean Time To Recover: the typical recovery duration. A target for the team, not a limit. |
| **Minimum operating level (MBCO)** | The reduced service level acceptable during an emergency (BSI *Notbetriebsniveau*). |
| **Degraded mode** | Running the service with reduced functionality instead of a full recovery. |
| **Tenant** | An organization using the tool. Tenants are isolated from each other. |
| **IT service** | A business-facing service. Owns the BIA, scenarios and the DR plan. |
| **Component** (API: microservice) | A part of an IT service that is recovered separately (database, front end, VM, SaaS). Owns the recovery objectives, strategies and runbooks. Optional: each service has a **default component** that stands for the whole service. |
| **Scenario** | A disaster or failure case (for example a region outage). It is brainstormed, then consolidated, then selected. |
| **Recovery strategy** | How a component recovers in a scenario (backup/restore, failover, and others). |
| **Gap check** | Comparison of the required objective with the estimated capability of a strategy (BSI *Soll-Ist-Vergleich*). |
| **ISCP** | Information System Contingency Plan (NIST SP 800-34). |
| **Activation / Recovery / Reconstitution** | The three plan phases in NIST SP 800-34, used as runbook step phases. |
| **FIPS 199 impact level** | NIST availability impact: low, moderate or high. |

## BSI terms (German)

| BSI term | English / meaning |
|----------|-------------------|
| **BCMS** | Business Continuity Management System (BSI 200-4 stages: Reaktiv, Aufbau, Standard) |
| **BAO** (*Besondere Aufbauorganisation*) | The special emergency or crisis organization (crisis staff, roles) |
| **MTA** (*Maximal tolerierbare Ausfallzeit*) | MTPD |
| **WAZ** (*Wiederanlaufzeit*) | RTO |
| **MTDV** (*Maximal tolerierbarer Datenverlust*) | RPO |
| **Notbetriebsniveau** | Minimum operating level |
| **Notbetrieb / Wiederanlauf / Wiederherstellung** | Emergency operation / restart / restoration to normal operation |
| **Notfallhandbuch** | Emergency handbook |
| **Wiederanlaufplan / Wiederherstellungsplan** | Restart and restoration plans (IT recovery procedures) |
| **Soll-Ist-Vergleich** | Comparison of required and actual capability (gap check) |
| **Schutzbedarf (Verfügbarkeit)** | Protection requirement for availability: normal, high or very high |
| **Vertretung** | Deputy |
