//! Built-in scenario templates for common network testing patterns.
//!
//! This module provides a collection of pre-defined scenario templates
//! for typical network testing scenarios like mobile device simulation,
//! network congestion, and intermittent connectivity issues.

use std::collections::HashMap;
use tracing::info;

use tcgui_shared::{
    scenario::{NetworkScenario, ScenarioMetadata, ScenarioStep},
    TcNetemConfig,
};

/// Manager for built-in scenario templates
pub struct BuiltinScenarioTemplates {
    templates: HashMap<String, NetworkScenario>,
}

impl Default for BuiltinScenarioTemplates {
    fn default() -> Self {
        Self::new()
    }
}

impl BuiltinScenarioTemplates {
    /// Create a new template manager with all built-in scenarios
    pub fn new() -> Self {
        info!("Loading built-in scenario templates");

        let mut templates = HashMap::new();

        // Add all built-in templates
        templates.insert(
            "mobile-degradation".to_string(),
            Self::create_mobile_degradation_template(),
        );
        templates.insert(
            "network-congestion".to_string(),
            Self::create_network_congestion_template(),
        );
        templates.insert(
            "intermittent-connectivity".to_string(),
            Self::create_intermittent_connectivity_template(),
        );
        templates.insert(
            "quality-degradation".to_string(),
            Self::create_quality_degradation_template(),
        );
        templates.insert(
            "load-testing".to_string(),
            Self::create_load_testing_template(),
        );
        templates.insert(
            "fast-degradation".to_string(),
            Self::create_fast_degradation_template(),
        );

        info!("Loaded {} built-in scenario templates", templates.len());

        Self { templates }
    }

    /// Get all available templates
    pub fn get_all_templates(&self) -> Vec<NetworkScenario> {
        self.templates.values().cloned().collect()
    }

    /// Get a specific template by ID
    pub fn get_template(&self, id: &str) -> Option<NetworkScenario> {
        self.templates.get(id).cloned()
    }

    /// Get template IDs
    pub fn get_template_ids(&self) -> Vec<String> {
        self.templates.keys().cloned().collect()
    }

    /// Mobile Device Moving Away Template
    fn create_mobile_degradation_template() -> NetworkScenario {
        let mut scenario = NetworkScenario::new(
            "mobile-degradation".to_string(),
            "Mobile Device Distance Simulation".to_string(),
            "Simulate mobile device moving away from base station with progressive signal degradation".to_string(),
        );

        scenario.metadata = ScenarioMetadata {
            tags: vec![
                "mobile".to_string(),
                "wireless".to_string(),
                "degradation".to_string(),
            ],
            author: Some("TC GUI Built-in Templates".to_string()),
            version: "1.0".to_string(),
            is_template: true,
            duration_ms: 120000, // 2 minutes
        };

        // Step 1: Close to base station - excellent signal
        let mut excellent_config = TcNetemConfig::new();
        excellent_config.delay.enabled = true;
        excellent_config.delay.base_ms = 5.0;

        scenario.add_step(ScenarioStep::new(
            0,
            "Close to base station - excellent signal quality".to_string(),
            excellent_config,
        ));

        // Step 2: Moving away - signal degradation starts
        let mut degrading_config = TcNetemConfig::new();
        degrading_config.loss.enabled = true;
        degrading_config.loss.percentage = 1.0;
        degrading_config.delay.enabled = true;
        degrading_config.delay.base_ms = 20.0;

        scenario.add_step(
            ScenarioStep::new(
                30000, // 30 seconds
                "Moving away - signal degradation begins".to_string(),
                degrading_config,
            )
            .with_linear_transition(10000),
        ); // 10 second smooth transition

        // Step 3: Far from base station - poor signal
        let mut poor_config = TcNetemConfig::new();
        poor_config.loss.enabled = true;
        poor_config.loss.percentage = 15.0;
        poor_config.loss.correlation = 25.0;
        poor_config.delay.enabled = true;
        poor_config.delay.base_ms = 100.0;
        poor_config.delay.jitter_ms = 50.0;

        scenario.add_step(
            ScenarioStep::new(
                60000, // 1 minute
                "Far from base station - poor signal with high latency".to_string(),
                poor_config,
            )
            .with_linear_transition(15000),
        ); // 15 second gradual degradation

        // Step 4: Very poor connection - edge of coverage
        let mut edge_config = TcNetemConfig::new();
        edge_config.loss.enabled = true;
        edge_config.loss.percentage = 25.0;
        edge_config.loss.correlation = 30.0;
        edge_config.delay.enabled = true;
        edge_config.delay.base_ms = 200.0;
        edge_config.delay.jitter_ms = 100.0;
        edge_config.duplicate.enabled = true;
        edge_config.duplicate.percentage = 1.0;

        scenario.add_step(
            ScenarioStep::new(
                90000, // 1.5 minutes
                "Edge of coverage - very poor connection".to_string(),
                edge_config,
            )
            .with_exponential_transition(20000),
        ); // 20 second exponential degradation

        scenario.recalculate_duration();
        scenario
    }

    /// Network Congestion Pattern Template
    fn create_network_congestion_template() -> NetworkScenario {
        let mut scenario = NetworkScenario::new(
            "network-congestion".to_string(),
            "Network Congestion Simulation".to_string(),
            "Simulate daily network usage patterns with varying congestion levels".to_string(),
        );

        scenario.metadata = ScenarioMetadata {
            tags: vec![
                "congestion".to_string(),
                "bandwidth".to_string(),
                "daily-pattern".to_string(),
            ],
            author: Some("TC GUI Built-in Templates".to_string()),
            version: "1.0".to_string(),
            is_template: true,
            duration_ms: 300000, // 5 minutes (compressed daily cycle)
        };

        // Step 1: Off-peak hours - low congestion
        let mut off_peak_config = TcNetemConfig::new();
        off_peak_config.loss.enabled = true;
        off_peak_config.loss.percentage = 0.1;
        off_peak_config.delay.enabled = true;
        off_peak_config.delay.base_ms = 10.0;

        scenario.add_step(
            ScenarioStep::new(
                0,
                "Off-peak hours - minimal network congestion".to_string(),
                off_peak_config,
            )
            .with_duration(60000),
        ); // Hold for 1 minute

        // Step 2: Morning rush - increased congestion
        let mut morning_rush_config = TcNetemConfig::new();
        morning_rush_config.loss.enabled = true;
        morning_rush_config.loss.percentage = 2.0;
        morning_rush_config.delay.enabled = true;
        morning_rush_config.delay.base_ms = 50.0;
        morning_rush_config.delay.jitter_ms = 20.0;
        morning_rush_config.duplicate.enabled = true;
        morning_rush_config.duplicate.percentage = 0.5;

        scenario.add_step(
            ScenarioStep::new(
                60000, // 1 minute
                "Morning peak - increased network congestion".to_string(),
                morning_rush_config,
            )
            .with_linear_transition(30000)
            .with_duration(90000),
        ); // 30s transition, hold 1.5 minutes

        // Step 3: Peak congestion - maximum load
        let mut peak_config = TcNetemConfig::new();
        peak_config.loss.enabled = true;
        peak_config.loss.percentage = 5.0;
        peak_config.loss.correlation = 15.0;
        peak_config.delay.enabled = true;
        peak_config.delay.base_ms = 150.0;
        peak_config.delay.jitter_ms = 75.0;
        peak_config.duplicate.enabled = true;
        peak_config.duplicate.percentage = 1.5;
        peak_config.rate_limit.enabled = true;
        peak_config.rate_limit.rate_kbps = 1000; // 1 Mbps limit

        scenario.add_step(
            ScenarioStep::new(
                180000, // 3 minutes
                "Peak congestion - maximum network load".to_string(),
                peak_config,
            )
            .with_exponential_transition(20000)
            .with_duration(60000),
        ); // 20s exponential increase, hold 1 minute

        // Step 4: Recovery - congestion decreasing
        let mut recovery_config = TcNetemConfig::new();
        recovery_config.loss.enabled = true;
        recovery_config.loss.percentage = 1.0;
        recovery_config.delay.enabled = true;
        recovery_config.delay.base_ms = 30.0;
        recovery_config.delay.jitter_ms = 10.0;

        scenario.add_step(
            ScenarioStep::new(
                260000, // 4 minutes 20 seconds
                "Recovery period - congestion decreasing".to_string(),
                recovery_config,
            )
            .with_linear_transition(40000),
        ); // 40s gradual recovery

        scenario.recalculate_duration();
        scenario
    }

    /// Intermittent Connectivity Template
    fn create_intermittent_connectivity_template() -> NetworkScenario {
        let mut scenario = NetworkScenario::new(
            "intermittent-connectivity".to_string(),
            "Intermittent Connectivity Simulation".to_string(),
            "Simulate unstable connection with sporadic drops and recoveries".to_string(),
        );

        scenario.metadata = ScenarioMetadata {
            tags: vec![
                "intermittent".to_string(),
                "unstable".to_string(),
                "drops".to_string(),
            ],
            author: Some("TC GUI Built-in Templates".to_string()),
            version: "1.0".to_string(),
            is_template: true,
            duration_ms: 180000, // 3 minutes
        };

        // Step 1: Normal operation
        let mut normal_config = TcNetemConfig::new();
        normal_config.delay.enabled = true;
        normal_config.delay.base_ms = 15.0;

        scenario.add_step(
            ScenarioStep::new(
                0,
                "Normal operation - stable connection".to_string(),
                normal_config,
            )
            .with_duration(20000),
        ); // 20 seconds

        // Step 2: First connectivity issue
        let mut issue1_config = TcNetemConfig::new();
        issue1_config.loss.enabled = true;
        issue1_config.loss.percentage = 30.0;
        issue1_config.delay.enabled = true;
        issue1_config.delay.base_ms = 500.0;
        issue1_config.delay.jitter_ms = 200.0;

        scenario.add_step(
            ScenarioStep::new(
                20000,
                "First connectivity issue - high packet loss".to_string(),
                issue1_config,
            )
            .with_duration(15000),
        ); // 15 seconds of issues

        // Step 3: Brief recovery
        let mut recovery1_config = TcNetemConfig::new();
        recovery1_config.loss.enabled = true;
        recovery1_config.loss.percentage = 2.0;
        recovery1_config.delay.enabled = true;
        recovery1_config.delay.base_ms = 50.0;

        scenario.add_step(
            ScenarioStep::new(
                40000,
                "Brief recovery - connection stabilizing".to_string(),
                recovery1_config,
            )
            .with_duration(25000),
        ); // 25 seconds recovery

        // Step 4: Severe drop
        let mut severe_config = TcNetemConfig::new();
        severe_config.loss.enabled = true;
        severe_config.loss.percentage = 60.0;
        severe_config.loss.correlation = 40.0;
        severe_config.delay.enabled = true;
        severe_config.delay.base_ms = 1000.0;
        severe_config.delay.jitter_ms = 500.0;
        severe_config.duplicate.enabled = true;
        severe_config.duplicate.percentage = 5.0;

        scenario.add_step(
            ScenarioStep::new(
                70000,
                "Severe connectivity drop - major packet loss".to_string(),
                severe_config,
            )
            .with_duration(20000),
        ); // 20 seconds severe issues

        // Step 5: Gradual recovery
        let mut gradual_recovery_config = TcNetemConfig::new();
        gradual_recovery_config.loss.enabled = true;
        gradual_recovery_config.loss.percentage = 5.0;
        gradual_recovery_config.delay.enabled = true;
        gradual_recovery_config.delay.base_ms = 80.0;
        gradual_recovery_config.delay.jitter_ms = 30.0;

        scenario.add_step(
            ScenarioStep::new(
                100000,
                "Gradual recovery - connection improving".to_string(),
                gradual_recovery_config,
            )
            .with_linear_transition(30000)
            .with_duration(40000),
        ); // 30s gradual improvement, hold 40s

        // Step 6: Final stability
        let mut stable_config = TcNetemConfig::new();
        stable_config.delay.enabled = true;
        stable_config.delay.base_ms = 20.0;

        scenario.add_step(
            ScenarioStep::new(
                170000,
                "Stable connection restored".to_string(),
                stable_config,
            )
            .with_linear_transition(10000),
        ); // 10s final stabilization

        scenario.recalculate_duration();
        scenario
    }

    /// Quality Degradation Template (gradual)
    fn create_quality_degradation_template() -> NetworkScenario {
        let mut scenario = NetworkScenario::new(
            "quality-degradation".to_string(),
            "Gradual Quality Degradation".to_string(),
            "Simulate gradual network quality degradation and recovery patterns".to_string(),
        );

        scenario.metadata = ScenarioMetadata {
            tags: vec![
                "degradation".to_string(),
                "gradual".to_string(),
                "recovery".to_string(),
            ],
            author: Some("TC GUI Built-in Templates".to_string()),
            version: "1.0".to_string(),
            is_template: true,
            duration_ms: 240000, // 4 minutes
        };

        // Create gradual degradation steps
        let degradation_steps = vec![
            (0, 0.0, 10.0, "Excellent quality"),
            (30000, 1.0, 25.0, "Minor degradation"),
            (60000, 3.0, 50.0, "Noticeable issues"),
            (90000, 7.0, 100.0, "Significant problems"),
            (120000, 12.0, 200.0, "Poor quality"),
            (150000, 8.0, 150.0, "Beginning recovery"),
            (180000, 4.0, 80.0, "Quality improving"),
            (210000, 1.0, 30.0, "Near excellent again"),
        ];

        for (timestamp, loss_percent, delay_ms, description) in degradation_steps {
            let mut config = TcNetemConfig::new();
            if loss_percent > 0.0 {
                config.loss.enabled = true;
                config.loss.percentage = loss_percent;
                config.loss.correlation = (loss_percent * 2.0).min(50.0);
            }

            config.delay.enabled = true;
            config.delay.base_ms = delay_ms;
            config.delay.jitter_ms = delay_ms * 0.3; // 30% jitter

            if loss_percent > 5.0 {
                config.duplicate.enabled = true;
                config.duplicate.percentage = (loss_percent * 0.2).min(2.0);
            }

            scenario.add_step(
                ScenarioStep::new(timestamp, description.to_string(), config)
                    .with_linear_transition(15000),
            ); // Smooth 15s transitions
        }

        scenario.recalculate_duration();
        scenario
    }

    /// Load Testing Template
    fn create_load_testing_template() -> NetworkScenario {
        let mut scenario = NetworkScenario::new(
            "load-testing".to_string(),
            "Load Testing Conditions".to_string(),
            "Reproducible network conditions for application load testing scenarios".to_string(),
        );

        scenario.metadata = ScenarioMetadata {
            tags: vec![
                "load-testing".to_string(),
                "reproducible".to_string(),
                "testing".to_string(),
            ],
            author: Some("TC GUI Built-in Templates".to_string()),
            version: "1.0".to_string(),
            is_template: true,
            duration_ms: 300000, // 5 minutes
        };

        // Baseline conditions
        let mut baseline_config = TcNetemConfig::new();
        baseline_config.delay.enabled = true;
        baseline_config.delay.base_ms = 50.0;

        scenario.add_step(
            ScenarioStep::new(
                0,
                "Baseline conditions - establish normal operation".to_string(),
                baseline_config,
            )
            .with_duration(60000),
        ); // 1 minute baseline

        // Light load conditions
        let mut light_load_config = TcNetemConfig::new();
        light_load_config.loss.enabled = true;
        light_load_config.loss.percentage = 0.5;
        light_load_config.delay.enabled = true;
        light_load_config.delay.base_ms = 75.0;
        light_load_config.delay.jitter_ms = 15.0;

        scenario.add_step(
            ScenarioStep::new(
                60000,
                "Light load conditions".to_string(),
                light_load_config,
            )
            .with_duration(60000),
        );

        // Medium load conditions
        let mut medium_load_config = TcNetemConfig::new();
        medium_load_config.loss.enabled = true;
        medium_load_config.loss.percentage = 2.0;
        medium_load_config.delay.enabled = true;
        medium_load_config.delay.base_ms = 120.0;
        medium_load_config.delay.jitter_ms = 40.0;
        medium_load_config.duplicate.enabled = true;
        medium_load_config.duplicate.percentage = 0.8;

        scenario.add_step(
            ScenarioStep::new(
                120000,
                "Medium load conditions".to_string(),
                medium_load_config,
            )
            .with_duration(60000),
        );

        // Heavy load conditions
        let mut heavy_load_config = TcNetemConfig::new();
        heavy_load_config.loss.enabled = true;
        heavy_load_config.loss.percentage = 5.0;
        heavy_load_config.loss.correlation = 20.0;
        heavy_load_config.delay.enabled = true;
        heavy_load_config.delay.base_ms = 200.0;
        heavy_load_config.delay.jitter_ms = 80.0;
        heavy_load_config.duplicate.enabled = true;
        heavy_load_config.duplicate.percentage = 1.5;
        heavy_load_config.rate_limit.enabled = true;
        heavy_load_config.rate_limit.rate_kbps = 2000;

        scenario.add_step(
            ScenarioStep::new(
                180000,
                "Heavy load conditions".to_string(),
                heavy_load_config,
            )
            .with_duration(60000),
        );

        // Recovery to baseline
        let mut recovery_config = TcNetemConfig::new();
        recovery_config.delay.enabled = true;
        recovery_config.delay.base_ms = 50.0;

        scenario.add_step(
            ScenarioStep::new(
                240000,
                "Recovery to baseline conditions".to_string(),
                recovery_config,
            )
            .with_linear_transition(30000),
        ); // 30s gradual recovery

        scenario.recalculate_duration();
        scenario
    }

    /// Fast Network Degradation Template (for testing/demonstration)
    fn create_fast_degradation_template() -> NetworkScenario {
        let mut scenario = NetworkScenario::new(
            "fast-degradation".to_string(),
            "Fast Network Degradation Test".to_string(),
            "Rapid network quality changes for testing and demonstration - shows quick progression through different network conditions".to_string(),
        );

        scenario.metadata = ScenarioMetadata {
            tags: vec![
                "testing".to_string(),
                "demo".to_string(),
                "fast".to_string(),
                "degradation".to_string(),
            ],
            author: Some("TC GUI Built-in Templates".to_string()),
            version: "1.0".to_string(),
            is_template: true,
            duration_ms: 30000, // 30 seconds total
        };

        // Step 1: Perfect connection (0-3 seconds)
        let perfect_config = TcNetemConfig::new();
        // No TC parameters - perfect connection

        scenario.add_step(ScenarioStep::new(
            0,
            "Perfect connection - no network impairment".to_string(),
            perfect_config,
        ));

        // Step 2: Light packet loss (3-6 seconds)
        let mut light_loss_config = TcNetemConfig::new();
        light_loss_config.loss.enabled = true;
        light_loss_config.loss.percentage = 2.0;

        scenario.add_step(ScenarioStep::new(
            3000,
            "Light packet loss - 2% loss rate".to_string(),
            light_loss_config,
        ));

        // Step 3: Add latency (6-9 seconds)
        let mut latency_config = TcNetemConfig::new();
        latency_config.loss.enabled = true;
        latency_config.loss.percentage = 5.0;
        latency_config.delay.enabled = true;
        latency_config.delay.base_ms = 100.0;

        scenario.add_step(ScenarioStep::new(
            6000,
            "High latency - 100ms delay with 5% loss".to_string(),
            latency_config,
        ));

        // Step 4: Jitter and correlation (9-12 seconds)
        let mut jitter_config = TcNetemConfig::new();
        jitter_config.loss.enabled = true;
        jitter_config.loss.percentage = 8.0;
        jitter_config.loss.correlation = 25.0;
        jitter_config.delay.enabled = true;
        jitter_config.delay.base_ms = 150.0;
        jitter_config.delay.jitter_ms = 50.0;

        scenario.add_step(ScenarioStep::new(
            9000,
            "Unstable connection - jitter and correlated loss".to_string(),
            jitter_config,
        ));

        // Step 5: Packet duplication (12-15 seconds)
        let mut duplicate_config = TcNetemConfig::new();
        duplicate_config.loss.enabled = true;
        duplicate_config.loss.percentage = 12.0;
        duplicate_config.delay.enabled = true;
        duplicate_config.delay.base_ms = 200.0;
        duplicate_config.duplicate.enabled = true;
        duplicate_config.duplicate.percentage = 3.0;

        scenario.add_step(ScenarioStep::new(
            12000,
            "Packet duplication - 3% duplicate rate".to_string(),
            duplicate_config,
        ));

        // Step 6: Reordering (15-18 seconds)
        let mut reorder_config = TcNetemConfig::new();
        reorder_config.loss.enabled = true;
        reorder_config.loss.percentage = 15.0;
        reorder_config.delay.enabled = true;
        reorder_config.delay.base_ms = 250.0;
        reorder_config.reorder.enabled = true;
        reorder_config.reorder.percentage = 5.0;
        reorder_config.reorder.gap = 3;

        scenario.add_step(ScenarioStep::new(
            15000,
            "Packet reordering - 5% reorder rate".to_string(),
            reorder_config,
        ));

        // Step 7: Corruption (18-21 seconds)
        let mut corrupt_config = TcNetemConfig::new();
        corrupt_config.loss.enabled = true;
        corrupt_config.loss.percentage = 20.0;
        corrupt_config.delay.enabled = true;
        corrupt_config.delay.base_ms = 300.0;
        corrupt_config.corrupt.enabled = true;
        corrupt_config.corrupt.percentage = 2.0;

        scenario.add_step(ScenarioStep::new(
            18000,
            "Packet corruption - 2% corrupt rate".to_string(),
            corrupt_config,
        ));

        // Step 8: Rate limiting (21-24 seconds)
        let mut rate_limit_config = TcNetemConfig::new();
        rate_limit_config.loss.enabled = true;
        rate_limit_config.loss.percentage = 25.0;
        rate_limit_config.delay.enabled = true;
        rate_limit_config.delay.base_ms = 400.0;
        rate_limit_config.rate_limit.enabled = true;
        rate_limit_config.rate_limit.rate_kbps = 1000; // 1 Mbps limit

        scenario.add_step(ScenarioStep::new(
            21000,
            "Rate limiting - 1 Mbps bandwidth limit".to_string(),
            rate_limit_config,
        ));

        // Step 9: Severe degradation - everything enabled (24-27 seconds)
        let mut severe_config = TcNetemConfig::new();
        severe_config.loss.enabled = true;
        severe_config.loss.percentage = 30.0;
        severe_config.loss.correlation = 50.0;
        severe_config.delay.enabled = true;
        severe_config.delay.base_ms = 500.0;
        severe_config.delay.jitter_ms = 200.0;
        severe_config.delay.correlation = 25.0;
        severe_config.duplicate.enabled = true;
        severe_config.duplicate.percentage = 5.0;
        severe_config.reorder.enabled = true;
        severe_config.reorder.percentage = 10.0;
        severe_config.reorder.gap = 5;
        severe_config.corrupt.enabled = true;
        severe_config.corrupt.percentage = 3.0;
        severe_config.rate_limit.enabled = true;
        severe_config.rate_limit.rate_kbps = 512; // 512 kbps limit

        scenario.add_step(ScenarioStep::new(
            24000,
            "Severe degradation - all impairments active".to_string(),
            severe_config,
        ));

        // Step 10: Recovery (27-30 seconds)
        let recovery_config = TcNetemConfig::new();
        // Back to perfect connection

        scenario.add_step(ScenarioStep::new(
            27000,
            "Network recovery - back to normal conditions".to_string(),
            recovery_config,
        ));

        scenario.recalculate_duration();
        scenario
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tcgui_shared::TcValidate;

    #[test]
    fn test_template_creation() {
        let templates = BuiltinScenarioTemplates::new();

        assert!(!templates.templates.is_empty());
        assert!(templates.get_template("mobile-degradation").is_some());
        assert!(templates.get_template("network-congestion").is_some());
        assert!(templates
            .get_template("intermittent-connectivity")
            .is_some());
        assert!(templates.get_template("nonexistent-template").is_none());
    }

    #[test]
    fn test_mobile_degradation_template() {
        let template = BuiltinScenarioTemplates::create_mobile_degradation_template();

        assert_eq!(template.id, "mobile-degradation");
        assert!(template.metadata.is_template);
        assert!(!template.steps.is_empty());
        assert!(template.validate().is_ok());

        // Check that degradation increases over time
        let first_step = &template.steps[0];
        let last_step = &template.steps[template.steps.len() - 1];

        assert!(last_step.tc_config.loss.percentage > first_step.tc_config.loss.percentage);
    }

    #[test]
    fn test_all_templates_validation() {
        let templates = BuiltinScenarioTemplates::new();

        for scenario in templates.get_all_templates() {
            assert!(
                scenario.validate().is_ok(),
                "Template '{}' failed validation: {:?}",
                scenario.id,
                scenario.validate()
            );
        }
    }

    #[test]
    fn test_template_metadata() {
        let templates = BuiltinScenarioTemplates::new();

        for scenario in templates.get_all_templates() {
            assert!(scenario.metadata.is_template);
            assert!(!scenario.metadata.tags.is_empty());
            assert!(scenario.metadata.author.is_some());
            assert!(!scenario.metadata.version.is_empty());
            assert!(scenario.metadata.duration_ms > 0);
        }
    }
}
