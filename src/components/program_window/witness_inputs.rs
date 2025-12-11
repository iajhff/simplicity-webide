use leptos::{view, For, IntoView, SignalGet, SignalUpdate};

#[derive(Debug, Clone, PartialEq)]
pub struct WitnessField {
    pub name: String,
    pub type_name: String,
    pub value: String,
    pub placeholder: String,
}

/// Parse witness declarations from program text
/// Looks for: const VAR_NAME: Type = ...
pub fn parse_witness_fields(program_text: &str) -> Vec<WitnessField> {
    let mut fields = Vec::new();
    
    // First, check if mod witness exists and parse it
    if let Some(start) = program_text.find("mod witness {") {
        let after_start = &program_text[start..];
        if let Some(end) = after_start.find('}') {
            let witness_block = &after_start[13..end]; // Skip "mod witness {"
            
            // Parse each const declaration: const NAME: TYPE = VALUE;
            let lines: Vec<&str> = witness_block.lines().collect();
            for line in lines {
                let trimmed = line.trim();
                if trimmed.starts_with("const ") {
                    // Extract name and type
                    if let Some(colon_pos) = trimmed.find(':') {
                        let name_part = &trimmed[6..colon_pos].trim(); // Skip "const "
                        
                        if let Some(eq_pos) = trimmed.find('=') {
                            let type_part = trimmed[colon_pos + 1..eq_pos].trim();
                            
                            let placeholder = match type_part {
                                "Signature" | "[u8; 64]" => "64-byte hex signature (128 chars)",
                                "Pubkey" | "[u8; 32]" => "32-byte hex pubkey (64 chars)",
                                "u32" => "Number (e.g. 100000)",
                                "u256" | "[u8; 32]" if name_part.contains("HASH") => "32-byte hex hash (64 chars)",
                                _ => "Value for this witness",
                            };
                            
                            fields.push(WitnessField {
                                name: name_part.to_string(),
                                type_name: type_part.to_string(),
                                value: String::new(),
                                placeholder: placeholder.to_string(),
                            });
                        }
                    }
                }
            }
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

