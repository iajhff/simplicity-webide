use leptos::{view, For, IntoView, SignalGet, SignalUpdate};

#[derive(Debug, Clone, PartialEq)]
pub struct WitnessField {
    pub name: String,
    pub type_name: String,
    pub value: String,
    pub placeholder: String,
}

/// Parse witness variable references from program text
/// Looks for: witness::VAR_NAME and infers type
pub fn parse_witness_fields(program_text: &str) -> Vec<WitnessField> {
    use regex::Regex;
    
    let mut fields = Vec::new();
    
    // Find all witness::VARIABLE_NAME references
    let witness_re = Regex::new(r"witness::([A-Z_][A-Z0-9_]*)").unwrap();
    
    for cap in witness_re.captures_iter(program_text) {
        let var_name = cap[1].to_string();
        
        // Infer type from variable name
        let (type_name, placeholder) = if var_name.contains("SIGNATURE") || var_name.ends_with("_SIG") || var_name == "SIG" {
            ("Signature", "Will be auto-generated from Key Store")
        } else if var_name.contains("PUBKEY") || var_name.contains("PUBLIC_KEY") || var_name == "PK" {
            ("Pubkey", "Will be auto-filled from Key Store")
        } else if var_name.contains("PRICE") || var_name.contains("HEIGHT") || var_name.contains("ORACLE") {
            ("u32", "Number (e.g. 50000)")
        } else if var_name.contains("HASH") {
            ("u256", "32-byte hex hash (64 hex chars)")
        } else if var_name.contains("PREIMAGE") || var_name.contains("SECRET") {
            ("u256", "32-byte hex value (64 hex chars)")
        } else {
            // Default to Signature for unknown types
            ("Signature", "Will be auto-generated from Key Store")
        };
        
        // Avoid duplicates
        if !fields.iter().any(|f: &WitnessField| f.name == var_name) {
            fields.push(WitnessField {
                name: var_name,
                type_name: type_name.to_string(),
                value: String::new(),
                placeholder: placeholder.to_string(),
            });
        }
    }
    
    fields
}

/// Generate mod witness block from witness fields
pub fn generate_witness_module(fields: &[WitnessField]) -> String {
    if fields.is_empty() {
        return String::new();
    }
    
    let mut result = String::from("mod witness {\n");
    
    for field in fields {
        if field.value.is_empty() {
            // Use placeholder value
            let default_value = match field.type_name.as_str() {
                "Signature" | "[u8; 64]" => "0x0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
                "Pubkey" | "[u8; 32]" => "0x0000000000000000000000000000000000000000000000000000000000000000",
                "u32" => "0",
                "u256" => "0x0000000000000000000000000000000000000000000000000000000000000000",
                _ => "/* TODO: provide value */",
            };
            result.push_str(&format!("    const {}: {} = {};\n", field.name, field.type_name, default_value));
        } else {
            result.push_str(&format!("    const {}: {} = {};\n", field.name, field.type_name, field.value));
        }
    }
    
    result.push_str("}\n\n");
    result
}

// Component removed - witness inputs are now inline in testnet_automation_button.rs

