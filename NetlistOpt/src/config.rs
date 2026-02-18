use std::fs;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct DesignConfig {
    pub is_finfet: bool,
    pub pmos_model: String,
    pub nmos_model: String,
    pub construction_tyle: ConstructionType,
    pub rewrite_stages: Vec<RewriteStageConfig>,
    pub default_rewrite_iter_limit: usize,
    pub extraction_start_depth: usize,
    pub extration_max_depth: usize,
    pub max_attempts_until_first_candidate: usize,
    pub uniform_sample_seed: u64,
    pub simulation_concurrency: usize,
}

#[derive(Debug, Clone)]
pub struct RewriteStageConfig {
    pub ruleset: String,
    pub iter_limit: usize,
}

impl Default for DesignConfig {
    fn default() -> Self {
        Self {
            is_finfet: false,
            pmos_model: "pmos_lvt".to_string(),
            nmos_model: "nmos_lvt".to_string(),
            construction_tyle: ConstructionType::default(),
            rewrite_stages: Vec::new(),
            default_rewrite_iter_limit: 3,
            extraction_start_depth: 4,
            extration_max_depth:9,
            max_attempts_until_first_candidate: 100000,
            uniform_sample_seed: 1234567,
            simulation_concurrency: 40,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConstructionType {
    Direct,
    JoinedCircuit,
}

impl Default for ConstructionType {
    fn default() -> Self {
        ConstructionType::Direct
    }
}

impl ConstructionType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ConstructionType::Direct => "direct",
            ConstructionType::JoinedCircuit => "joined_circuit",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "direct" => Some(ConstructionType::Direct),
            "joined_circuit" => Some(ConstructionType::JoinedCircuit),
            _ => None,
        }
    }
}

impl DesignConfig {
    pub fn load_from_path(path: &Path) -> Result<Self, String> {
        let text = fs::read_to_string(path)
            .map_err(|e| format!("failed to read config {:?}: {}", path, e))?;
        let mut cfg = DesignConfig::default();
        let mut stage_specs: Vec<(String, Option<usize>)> = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some((k, v)) = line.split_once('=') {
                let key = k.trim();
                let val = v.trim();
                if key.eq_ignore_ascii_case("is_finfet") {
                    match val {
                        "true" | "1" | "yes" | "on" => cfg.is_finfet = true,
                        "false" | "0" | "no" | "off" => cfg.is_finfet = false,
                        _ => {}
                    }
                } else if key.eq_ignore_ascii_case("pmos_model") {
                    cfg.pmos_model = val.to_string();
                } else if key.eq_ignore_ascii_case("nmos_model") {
                    cfg.nmos_model = val.to_string();
                } else if key.eq_ignore_ascii_case("construction_tyle")
                    || key.eq_ignore_ascii_case("construction_type")
                {
                    if let Some(parsed) = ConstructionType::parse(val) {
                        cfg.construction_tyle = parsed;
                    }
                } else if key.eq_ignore_ascii_case("default_rewrite_iter_limit") {
                    if let Ok(limit) = val.parse::<usize>() {
                        cfg.default_rewrite_iter_limit = limit;
                    }
                } else if key.eq_ignore_ascii_case("rewrite_stage") {
                    if let Some(spec) = parse_rewrite_stage_spec(val) {
                        stage_specs.push(spec);
                    }
                } else if key.eq_ignore_ascii_case("extration_start_depth")
                    || key.eq_ignore_ascii_case("extraction_start_depth")
                {
                    if let Ok(depth) = val.parse::<usize>() {
                        cfg.extraction_start_depth = depth;
                    }
                } else if key.eq_ignore_ascii_case("extration_max_depth")
                    || key.eq_ignore_ascii_case("extraction_max_depth")
                {
                    if let Ok(depth) = val.parse::<usize>() {
                        cfg.extration_max_depth = depth;
                    }
                } else if key.eq_ignore_ascii_case("max_attempts_until_first_candidate")
                    || key.eq_ignore_ascii_case("max_attempts_till_first_candidate")
                {
                    if let Ok(limit) = val.parse::<usize>() {
                        cfg.max_attempts_until_first_candidate = limit;
                    }
                } else if key.eq_ignore_ascii_case("uniform_sample_seed") {
                    if let Ok(seed) = val.parse::<u64>() {
                        cfg.uniform_sample_seed = seed;
                    }
                } else if key.eq_ignore_ascii_case("simulation_concurrency") {
                    if let Ok(concurrency) = val.parse::<usize>() {
                        cfg.simulation_concurrency = concurrency;
                    }
                }
            }
        }
        if !stage_specs.is_empty() {
            cfg.rewrite_stages = stage_specs
                .into_iter()
                .map(|(ruleset, iter_limit)| RewriteStageConfig {
                    ruleset,
                    iter_limit: iter_limit
                        .unwrap_or(cfg.default_rewrite_iter_limit),
                })
                .collect();
        }
        Ok(cfg)
    }

    pub fn save_to_path(&self, path: &Path) -> Result<(), String> {
        let mut contents = format!(
            "is_finfet = {}\npmos_model = {}\nnmos_model = {}\nconstruction_tyle = {}\n",
            self.is_finfet,
            self.pmos_model,
            self.nmos_model,
            self.construction_tyle.as_str()
        );
        contents.push_str(&format!(
            "default_rewrite_iter_limit = {}\n",
            self.default_rewrite_iter_limit
        ));
        contents.push_str(&format!(
            "extraction_start_depth = {}\n",
            self.extraction_start_depth
        ));
        contents.push_str(&format!(
            "extration_max_depth = {}\n",
            self.extration_max_depth
        ));
        contents.push_str(&format!(
            "max_attempts_until_first_candidate = {}\n",
            self.max_attempts_until_first_candidate
        ));
        contents.push_str(&format!(
            "uniform_sample_seed = {}\n",
            self.uniform_sample_seed
        ));
        contents.push_str(&format!(
            "simulation_concurrency = {}\n",
            self.simulation_concurrency
        ));
        for stage in &self.rewrite_stages {
            contents.push_str(&format!(
                "rewrite_stage = {}:{}\n",
                stage.ruleset,
                stage.iter_limit
            ));
        }
        fs::write(path, contents)
            .map_err(|e| format!("failed to write config {:?}: {}", path, e))
    }

    pub fn resolved_rewrite_stages(&self) -> Vec<RewriteStageConfig> {
        if self.rewrite_stages.is_empty() {
            vec![RewriteStageConfig {
                ruleset: "default".to_string(),
                iter_limit: self.default_rewrite_iter_limit,
            }]
        } else {
            self.rewrite_stages.clone()
        }
    }
}

fn parse_rewrite_stage_spec(raw: &str) -> Option<(String, Option<usize>)> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some((name, limit)) = trimmed.split_once(':') {
        let name = name.trim();
        if name.is_empty() {
            return None;
        }
        let limit = limit.trim();
        if limit.is_empty() {
            return Some((name.to_string(), None));
        }
        let parsed = limit.parse::<usize>().ok()?;
        return Some((name.to_string(), Some(parsed)));
    }
    Some((trimmed.to_string(), None))
}
