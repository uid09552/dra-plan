---
title: "ADR-0005: NIST SP 800-34 and BSI 200-4 as methodological basis"
type: adr
status: accepted
tags: [adr, standards, nist, bsi]
created: 2026-09-27
updated: 2026-09-27
related: [standards-mapping, plan-authoring]
---

# ADR-0005: NIST SP 800-34 and BSI 200-4 as methodological basis

## Context
DR plans have to be defensible to auditors and familiar to practitioners. Users work in both international and German (BSI IT-Grundschutz) contexts.

## Decision
Base the workflow, terminology and data model on **NIST SP 800-34 Rev. 1** and **BSI-Standard 200-4** (with IT-Grundschutz DER.4 and CON.3). The mapping is kept in [[standards-mapping]]. The UI uses the English terms and shows the BSI equivalents where they are useful.

## Consequences
- The workflow steps and gates can be traced to both standards, which supports audit evidence.
- The terms (MTPD/MTA, RTO/WAZ, RPO/MTDV, Notbetriebsniveau) are covered in the glossary.
- ISO 22301 / ISO/IEC 27031 alignment can be added later through the same mapping document.
