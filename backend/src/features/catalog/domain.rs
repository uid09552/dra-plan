//! Static seed data for the workflow (docs/domain/scenario-catalog.md), in English and German.

use crate::features::dependencies::domain::{Dependency, DependencyKind};
use crate::features::directory::domain::DEFAULT_ROLES;
use crate::features::dr_tests::domain::DrTestType;
use crate::features::it_services::domain::{ImpactLevel, ItService, ProtectionRequirement};
use crate::features::microservices::domain::Microservice;
use crate::features::scenarios::domain::ScenarioCategory;
use crate::features::strategies::domain::StrategyType;
use crate::features::workflow::domain::{STEP_DEFINITIONS, StepDefinition};
use crate::shared::kernel::Language;

/// Text in both supported languages.
#[derive(Debug, Clone, Copy)]
pub struct Text {
    pub en: &'static str,
    pub de: &'static str,
}

impl Text {
    pub fn get(&self, lang: Language) -> &'static str {
        match lang {
            Language::En => self.en,
            Language::De => self.de,
        }
    }
}

const fn t(en: &'static str, de: &'static str) -> Text {
    Text { en, de }
}

pub fn category_label(c: ScenarioCategory) -> Text {
    match c {
        C::Infrastructure => t("Infrastructure", "Infrastruktur"),
        C::Hardware => t("Hardware", "Hardware"),
        C::Network => t("Network", "Netzwerk"),
        C::Cloud => t("Cloud", "Cloud"),
        C::Application => t("Application", "Anwendung"),
        C::Database => t("Database", "Datenbank"),
        C::Storage => t("Storage", "Speicher"),
        C::Backup => t("Backup", "Datensicherung"),
        C::Cybersecurity => t("Cybersecurity", "Cybersicherheit"),
        C::People => t("People", "Personal"),
        C::Supplier => t("Suppliers / third parties", "Lieferanten / Dritte"),
        C::Facility => t("Facility", "Gebäude"),
        C::Power => t("Power / utilities", "Strom / Versorgung"),
        C::Environmental => t(
            "Environmental / natural disasters",
            "Umwelt / Naturkatastrophen",
        ),
        C::Operational => t("Operational errors", "Bedienfehler"),
    }
}

/// When a template is suggested for a service (rule-based brainstorming assistance).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Relevance {
    /// Relevant for every IT service.
    Always,
    /// The service has data stores.
    DataStores,
    /// Hosted in a cloud (hosting location or platform mentions a cloud provider or Kubernetes).
    Cloud,
    /// Depends on an external service or supplier.
    ExternalDependency,
    /// Depends on an identity provider.
    IdentityProvider,
    /// High availability requirement (likely a target of attacks such as DDoS).
    HighProtection,
}

impl Relevance {
    pub fn as_str(self) -> &'static str {
        match self {
            Relevance::Always => "always",
            Relevance::DataStores => "data_stores",
            Relevance::Cloud => "cloud",
            Relevance::ExternalDependency => "external_dependency",
            Relevance::IdentityProvider => "identity_provider",
            Relevance::HighProtection => "high_protection",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ScenarioTemplate {
    pub id: &'static str,
    pub category: ScenarioCategory,
    pub title: Text,
    pub description: Text,
    pub relevance: Relevance,
}

impl ScenarioTemplate {
    pub fn title(&self, lang: Language) -> &'static str {
        self.title.get(lang)
    }

    pub fn description(&self, lang: Language) -> &'static str {
        self.description.get(lang)
    }
}

const fn tpl(
    id: &'static str,
    category: ScenarioCategory,
    relevance: Relevance,
    title: Text,
    description: Text,
) -> ScenarioTemplate {
    ScenarioTemplate {
        id,
        category,
        title,
        description,
        relevance,
    }
}

use Relevance as R;
use ScenarioCategory as C;
pub const SCENARIO_TEMPLATES: &[ScenarioTemplate] = &[
    tpl(
        "site-outage",
        C::Infrastructure,
        R::Always,
        t(
            "Data center or region outage",
            "Ausfall Rechenzentrum oder Region",
        ),
        t(
            "The primary data center or cloud region is completely unavailable.",
            "Das primäre Rechenzentrum oder die Cloud-Region ist vollständig nicht verfügbar.",
        ),
    ),
    tpl(
        "server-failure",
        C::Hardware,
        R::Always,
        t("Server failure", "Serverausfall"),
        t(
            "A physical or virtual server hosting the service fails.",
            "Ein physischer oder virtueller Server des Dienstes fällt aus.",
        ),
    ),
    tpl(
        "network-outage",
        C::Network,
        R::Always,
        t("Network outage", "Netzwerkausfall"),
        t(
            "Loss of network connectivity between components or to users.",
            "Verlust der Netzwerkverbindung zwischen Komponenten oder zu Nutzern.",
        ),
    ),
    tpl(
        "dns-failure",
        C::Network,
        R::Always,
        t("DNS failure", "DNS-Ausfall"),
        t(
            "Name resolution fails or returns wrong records.",
            "Namensauflösung schlägt fehl oder liefert falsche Einträge.",
        ),
    ),
    tpl(
        "cloud-provider-outage",
        C::Cloud,
        R::Cloud,
        t("Cloud provider outage", "Ausfall Cloud-Anbieter"),
        t(
            "A cloud provider service used by the IT service fails.",
            "Ein genutzter Dienst des Cloud-Anbieters fällt aus.",
        ),
    ),
    tpl(
        "bad-deployment",
        C::Application,
        R::Always,
        t("Failed deployment", "Fehlerhaftes Deployment"),
        t(
            "A release or configuration change breaks the service.",
            "Ein Release oder eine Konfigurationsänderung legt den Dienst lahm.",
        ),
    ),
    tpl(
        "certificate-expiry",
        C::Application,
        R::Always,
        t(
            "Certificate or secret expiry",
            "Ablauf von Zertifikat oder Secret",
        ),
        t(
            "An expired certificate or secret blocks communication.",
            "Ein abgelaufenes Zertifikat oder Secret blockiert die Kommunikation.",
        ),
    ),
    tpl(
        "database-failure",
        C::Database,
        R::DataStores,
        t("Database failure", "Datenbankausfall"),
        t(
            "The primary database becomes unavailable.",
            "Die primäre Datenbank ist nicht verfügbar.",
        ),
    ),
    tpl(
        "data-corruption",
        C::Database,
        R::DataStores,
        t("Database corruption", "Datenbankkorruption"),
        t(
            "Data is corrupted by software faults or operator error.",
            "Daten werden durch Softwarefehler oder Bedienfehler beschädigt.",
        ),
    ),
    tpl(
        "storage-failure",
        C::Storage,
        R::DataStores,
        t("Storage failure", "Speicherausfall"),
        t(
            "Storage systems fail or become read-only.",
            "Speichersysteme fallen aus oder sind nur noch lesbar.",
        ),
    ),
    tpl(
        "backup-unusable",
        C::Backup,
        R::DataStores,
        t("Backup unusable", "Datensicherung unbrauchbar"),
        t(
            "Backups are missing, incomplete or cannot be restored.",
            "Sicherungen fehlen, sind unvollständig oder nicht wiederherstellbar.",
        ),
    ),
    tpl(
        "ransomware",
        C::Cybersecurity,
        R::Always,
        t("Ransomware", "Ransomware"),
        t(
            "Systems and data are encrypted by ransomware; a clean rebuild is required.",
            "Systeme und Daten werden durch Ransomware verschlüsselt; ein sauberer Wiederaufbau ist nötig.",
        ),
    ),
    tpl(
        "ddos",
        C::Cybersecurity,
        R::HighProtection,
        t("DDoS attack", "DDoS-Angriff"),
        t(
            "The service is overloaded by a distributed denial-of-service attack.",
            "Der Dienst wird durch einen verteilten Überlastungsangriff lahmgelegt.",
        ),
    ),
    tpl(
        "compromised-credentials",
        C::Cybersecurity,
        R::Always,
        t("Compromised credentials", "Kompromittierte Zugangsdaten"),
        t(
            "Privileged credentials are compromised.",
            "Privilegierte Zugangsdaten sind kompromittiert.",
        ),
    ),
    tpl(
        "key-personnel-loss",
        C::People,
        R::Always,
        t(
            "Critical administrator unavailable",
            "Kritischer Administrator nicht verfügbar",
        ),
        t(
            "People with critical knowledge are unavailable.",
            "Personen mit kritischem Wissen sind nicht verfügbar.",
        ),
    ),
    tpl(
        "identity-provider-down",
        C::Supplier,
        R::IdentityProvider,
        t(
            "Identity provider unavailable",
            "Identity Provider nicht verfügbar",
        ),
        t(
            "Users and services cannot authenticate.",
            "Nutzer und Dienste können sich nicht authentifizieren.",
        ),
    ),
    tpl(
        "external-api-outage",
        C::Supplier,
        R::ExternalDependency,
        t("Supplier or SaaS outage", "Ausfall Lieferant oder SaaS"),
        t(
            "A third-party API, SaaS or supplier the service depends on fails.",
            "Eine externe API, ein SaaS-Dienst oder Lieferant, von dem der Dienst abhängt, fällt aus.",
        ),
    ),
    tpl(
        "building-unavailable",
        C::Facility,
        R::Always,
        t("Building unavailable", "Gebäude nicht nutzbar"),
        t(
            "Offices or operations centers cannot be accessed.",
            "Büros oder Leitstellen sind nicht zugänglich.",
        ),
    ),
    tpl(
        "power-outage",
        C::Power,
        R::Always,
        t("Power outage", "Stromausfall"),
        t(
            "Loss of power at the hosting site beyond UPS capacity.",
            "Stromausfall am Standort über die USV-Kapazität hinaus.",
        ),
    ),
    tpl(
        "natural-disaster",
        C::Environmental,
        R::Always,
        t("Flood, fire or storm", "Hochwasser, Brand oder Sturm"),
        t(
            "A natural disaster damages or blocks the site.",
            "Eine Naturkatastrophe beschädigt oder blockiert den Standort.",
        ),
    ),
    tpl(
        "operator-error",
        C::Operational,
        R::Always,
        t("Operator error", "Bedienfehler"),
        t(
            "An administrator deletes or misconfigures critical resources.",
            "Ein Administrator löscht oder konfiguriert kritische Ressourcen falsch.",
        ),
    ),
];

pub fn scenario_template(id: &str) -> Option<&'static ScenarioTemplate> {
    SCENARIO_TEMPLATES.iter().find(|t| t.id == id)
}

/// Facts about a service that drive scenario suggestions.
#[derive(Debug, Clone, Default)]
pub struct ServiceProfile {
    pub has_data_stores: bool,
    pub cloud_hosted: bool,
    pub has_external_dependency: bool,
    pub has_identity_dependency: bool,
    pub high_protection: bool,
}

const CLOUD_KEYWORDS: &[&str] = &[
    "aws",
    "azure",
    "gcp",
    "google",
    "cloud",
    "kubernetes",
    "k8s",
    "openshift",
];
const IDENTITY_KEYWORDS: &[&str] = &[
    "idp",
    "identity",
    "keycloak",
    "entra",
    "active directory",
    "ldap",
    "sso",
    "auth",
];

fn mentions(text: Option<&str>, keywords: &[&str]) -> bool {
    text.is_some_and(|t| {
        let t = t.to_lowercase();
        keywords.iter().any(|k| t.contains(k))
    })
}

impl ServiceProfile {
    /// Derives the profile from what is already captured (steps 1–5): data stores, hosting,
    /// external dependencies and the protection requirement.
    pub fn derive(
        service: &ItService,
        microservices: &[Microservice],
        dependencies: &[Dependency],
    ) -> Self {
        let external = |d: &&Dependency| {
            matches!(
                d.kind,
                DependencyKind::ExternalService | DependencyKind::Supplier
            )
        };
        ServiceProfile {
            has_data_stores: microservices.iter().any(|m| !m.data_stores.is_empty()),
            cloud_hosted: microservices.iter().any(|m| {
                mentions(m.platform.as_deref(), CLOUD_KEYWORDS)
                    || mentions(m.hosting_location.as_deref(), CLOUD_KEYWORDS)
            }),
            has_external_dependency: dependencies.iter().any(|d| external(&d)),
            has_identity_dependency: dependencies
                .iter()
                .any(|d| mentions(d.target_name.as_deref(), IDENTITY_KEYWORDS)),
            high_protection: matches!(
                service.protection_requirement_availability,
                Some(ProtectionRequirement::High | ProtectionRequirement::VeryHigh)
            ) || service.impact_level == Some(ImpactLevel::High),
        }
    }

    pub fn matches(&self, r: Relevance) -> bool {
        match r {
            Relevance::Always => true,
            Relevance::DataStores => self.has_data_stores,
            Relevance::Cloud => self.cloud_hosted,
            Relevance::ExternalDependency => self.has_external_dependency,
            Relevance::IdentityProvider => self.has_identity_dependency,
            Relevance::HighProtection => self.high_protection,
        }
    }
}

/// Templates relevant for the service, excluding those already used (by template title).
pub fn suggest(
    profile: &ServiceProfile,
    existing_titles: &[String],
) -> Vec<&'static ScenarioTemplate> {
    let known = |t: &ScenarioTemplate| {
        existing_titles
            .iter()
            .any(|e| e.eq_ignore_ascii_case(t.title.en) || e.eq_ignore_ascii_case(t.title.de))
    };
    SCENARIO_TEMPLATES
        .iter()
        .filter(|t| profile.matches(t.relevance) && !known(t))
        .collect()
}

#[derive(Debug, Clone, Copy)]
pub struct StrategyTypeInfo {
    pub key: StrategyType,
    pub label: Text,
    pub typical_rto: Text,
    pub typical_rpo: Text,
    pub notes: Text,
}

pub const STRATEGY_TYPES: &[StrategyTypeInfo] = &[
    StrategyTypeInfo {
        key: StrategyType::BackupRestore,
        label: t("Backup and restore", "Sicherung und Wiederherstellung"),
        typical_rto: t("hours to days", "Stunden bis Tage"),
        typical_rpo: t("backup interval", "Sicherungsintervall"),
        notes: t(
            "Cheapest; requires tested restores (BSI CON.3).",
            "Am günstigsten; erfordert getestete Wiederherstellungen (BSI CON.3).",
        ),
    },
    StrategyTypeInfo {
        key: StrategyType::Replication,
        label: t("Replication", "Replikation"),
        typical_rto: t("minutes to hours", "Minuten bis Stunden"),
        typical_rpo: t("seconds to minutes", "Sekunden bis Minuten"),
        notes: t(
            "Also replicates corruption; combine with backups.",
            "Repliziert auch Korruption; mit Sicherungen kombinieren.",
        ),
    },
    StrategyTypeInfo {
        key: StrategyType::ActivePassive,
        label: t("Active/passive", "Aktiv/Passiv"),
        typical_rto: t("minutes to hours", "Minuten bis Stunden"),
        typical_rpo: t("seconds to minutes", "Sekunden bis Minuten"),
        notes: t("Warm or cold standby.", "Warm- oder Cold-Standby."),
    },
    StrategyTypeInfo {
        key: StrategyType::ActiveActive,
        label: t("Active/active", "Aktiv/Aktiv"),
        typical_rto: t("near zero", "nahezu null"),
        typical_rpo: t("near zero", "nahezu null"),
        notes: t(
            "Highest cost and complexity.",
            "Höchste Kosten und Komplexität.",
        ),
    },
    StrategyTypeInfo {
        key: StrategyType::CrossRegionFailover,
        label: t("Cross-region failover", "Regionsübergreifendes Failover"),
        typical_rto: t("minutes to hours", "Minuten bis Stunden"),
        typical_rpo: t("seconds to minutes", "Sekunden bis Minuten"),
        notes: t(
            "Requires DNS/traffic switching.",
            "Erfordert DNS-/Traffic-Umschaltung.",
        ),
    },
    StrategyTypeInfo {
        key: StrategyType::IacRebuild,
        label: t(
            "Rebuild from infrastructure as code",
            "Neuaufbau aus Infrastructure as Code",
        ),
        typical_rto: t("hours", "Stunden"),
        typical_rpo: t("depends on data", "abhängig von den Daten"),
        notes: t(
            "Preferred after a security compromise.",
            "Bevorzugt nach einer Kompromittierung.",
        ),
    },
    StrategyTypeInfo {
        key: StrategyType::DegradedMode,
        label: t("Degraded mode", "Notbetrieb"),
        typical_rto: t("immediate", "sofort"),
        typical_rpo: t("n/a", "n/a"),
        notes: t(
            "Minimum operating level (Notbetriebsniveau).",
            "Notbetriebsniveau.",
        ),
    },
    StrategyTypeInfo {
        key: StrategyType::ManualWorkaround,
        label: t("Manual workaround", "Manuelle Ersatzlösung"),
        typical_rto: t("varies", "variiert"),
        typical_rpo: t("n/a", "n/a"),
        notes: t(
            "Business process fallback.",
            "Ersatzprozess im Fachbereich.",
        ),
    },
];

pub fn test_type_label(t_: DrTestType) -> (Text, u8) {
    match t_ {
        DrTestType::PlanReview => (t("Plan review", "Planbesprechung"), 1),
        DrTestType::Tabletop => (t("Tabletop exercise", "Planspiel / Stabsübung"), 2),
        DrTestType::Simulation => (t("Simulation", "Simulation"), 3),
        DrTestType::TechnicalRecovery => (
            t("Technical recovery test", "Technischer Wiederanlauftest"),
            4,
        ),
        DrTestType::FullDr => (t("Full DR test", "Vollübung"), 5),
    }
}

pub fn default_roles() -> impl Iterator<Item = &'static str> {
    DEFAULT_ROLES.iter().map(|(name, _)| *name)
}

pub fn workflow_steps() -> &'static [StepDefinition] {
    STEP_DEFINITIONS
}
