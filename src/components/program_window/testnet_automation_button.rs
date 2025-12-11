use leptos::{component, create_rw_signal, create_signal, spawn_local, use_context, view, with, For, IntoView, SignalGet, SignalSet, SignalUpdate, SignalWith};
use web_sys::js_sys;
use simplicityhl::elements::secp256k1_zkp as secp256k1;
use hex_conservative::DisplayHex;

use crate::components::program_window::Program;
use crate::components::program_window::witness_inputs::{self, WitnessField};
use crate::components::run_window::{SignedData, TxEnv};
use crate::util::{self, SigningKeys};

/// Analyze program structure to infer contract type and requirements
fn analyze_program_structure(program_text: &str) -> ProgramAnalysis {
    let mut analysis = ProgramAnalysis {
        has_signature_verification: false,
        has_hash_check: false,
        has_timelock: false,
        has_conditional: false,
        signature_count: 0,
        uses_preimage: false,
    };
    
    // Detect signature verification
    if program_text.contains("jet::bip_0340_verify") {
        analysis.has_signature_verification = true;
        // Count how many times verify is called
        analysis.signature_count = program_text.matches("jet::bip_0340_verify").count();
    }
    
    // Detect hash operations (might need preimage)
    if program_text.contains("jet::sha_256") || program_text.contains("jet::eq_256") {
        analysis.has_hash_check = true;
    }
    
    // Detect timelock
    if program_text.contains("jet::check_lock_time") || program_text.contains("jet::check_sequence") {
        analysis.has_timelock = true;
    }
    
    // Detect conditional logic (might need sum types)
    if program_text.contains("match ") || program_text.contains("if ") {
        analysis.has_conditional = true;
    }
    
    // Detect preimage usage
    if program_text.contains("preimage") {
        analysis.uses_preimage = true;
    }
    
    analysis
}

#[derive(Debug)]
struct ProgramAnalysis {
    has_signature_verification: bool,
    has_hash_check: bool,
    has_timelock: bool,
    has_conditional: bool,
    signature_count: usize,
    uses_preimage: bool,
}

/// Parse witness variable declarations from mod witness block
fn parse_witness_declarations(program_text: &str) -> Vec<WitnessVariable> {
    let mut witness_vars = Vec::new();
    
    // First, check if mod witness exists
    if let Some(start) = program_text.find("mod witness {") {
        let after_start = &program_text[start..];
        if let Some(end) = after_start.find('}') {
            let witness_block = &after_start[13..end]; // Skip "mod witness {"
            
            // Parse each const declaration
            let const_re = regex::Regex::new(r"const\s+([A-Z_][A-Z0-9_]*)\s*:\s*([^=]+)=").unwrap();
            for cap in const_re.captures_iter(witness_block) {
                let var_name = cap[1].trim().to_string();
                let var_type = cap[2].trim().to_string();
                witness_vars.push(WitnessVariable {
                    name: var_name,
                    type_name: var_type,
                    value: String::new(),
                });
            }
        }
    } else {
        // No mod witness block - scan for witness:: references
        let witness_re = regex::Regex::new(r"witness::([A-Z_][A-Z0-9_]*)").unwrap();
        for cap in witness_re.captures_iter(program_text) {
            let var_name = cap[1].to_string();
            if !witness_vars.iter().any(|v| v.name == var_name) {
                witness_vars.push(WitnessVariable {
                    name: var_name,
                    type_name: "Unknown".to_string(),
                    value: String::new(),
                });
            }
        }
    }
    
    witness_vars
}

#[derive(Debug, Clone)]
struct WitnessVariable {
    name: String,
    type_name: String,
    value: String,
}

/// Infer witness variable type from name and program context
fn infer_witness_type(var_name: &str, analysis: &ProgramAnalysis, program_text: &str) -> String {
    // Check for multi-signature patterns
    if var_name.contains("2_OF_3") || var_name.contains("MULTISIG") || var_name.contains("_OF_") {
        return "MultiSig".to_string(); // Complex array of optional signatures
    }
    
    // Check variable name patterns - ONLY generate for explicit signature/key names
    if var_name.contains("SIGNATURE") || var_name == "SIG" || var_name.ends_with("_SIG") {
        // If multiple signatures are needed, it might be an array
        if analysis.signature_count > 1 {
            return "MultiSig".to_string();
        }
        return "Signature".to_string();
    }
    
    if var_name.contains("PUBLIC_KEY") || var_name.contains("PUBKEY") || var_name == "PK" {
        return "Pubkey".to_string();
    }
    
    if var_name.contains("PREIMAGE") || var_name.contains("SECRET") {
        return "u256".to_string(); // Preimages are typically u256
    }
    
    // Check for oracle or price data
    if var_name.contains("ORACLE") || var_name.contains("PRICE") || var_name.contains("HEIGHT") {
        return "u32".to_string(); // Oracle data is typically u32
    }
    
    // Check if it's used in a conditional (might be sum type)
    if analysis.has_conditional {
        // Look for the variable in match or if expressions
        let var_ref = format!("witness::{}", var_name);
        if program_text.contains(&format!("match {}", var_ref)) 
            || program_text.contains(&format!("unwrap_left({})", var_ref)) 
            || program_text.contains(&format!("unwrap_right({})", var_ref)) {
            return "SumType".to_string(); // Complex sum type - needs manual definition
        }
    }
    
    // Default to Unknown - don't assume type
    "Unknown".to_string()
}

/// Generate witness/param values based on detected variables
fn generate_witness_values(
    witness_vars: &[(String, String)],
    signing_keys: &SigningKeys,
    sighash: secp256k1::Message,
    selected_key_indices: &[usize],
) -> Result<(Vec<(String, String, String)>, bool), String> {
    use hex_conservative::DisplayHex;
    let mut generated = Vec::new();
    let mut unsupported = Vec::new();
    let mut needs_multisig_selection = false;
    
    for (var_name, var_type) in witness_vars {
        match var_type.as_str() {
            "Signature" => {
                // Generate signature using Alice's key (index 0)
                let sig = signing_keys.secret_keys[0].sign_schnorr(sighash);
                let sig_hex = format!("0x{}", sig.serialize().as_hex());
                generated.push((var_name.clone(), var_type.clone(), sig_hex));
            }
            "Pubkey" => {
                // Use Alice's public key
                let pk = signing_keys.secret_keys[0].x_only_public_key().0;
                let pk_hex = format!("0x{}", pk.serialize().as_hex());
                generated.push((var_name.clone(), var_type.clone(), pk_hex));
            }
            "MultiSig" => {
                // For multisig, generate individual signature params if keys are selected
                if selected_key_indices.is_empty() {
                    needs_multisig_selection = true;
                } else {
                    // Generate individual signatures for selected keys
                    for &key_idx in selected_key_indices {
                        let key_name = get_key_name(key_idx);
                        let sig = signing_keys.secret_keys[key_idx].sign_schnorr(sighash);
                        let sig_hex = format!("0x{}", sig.serialize().as_hex());
                        generated.push((
                            format!("{}_SIGNATURE", key_name.to_uppercase()),
                            "Signature".to_string(),
                            sig_hex
                        ));
                    }
                }
            }
            "u32" => {
                // Oracle data or other u32 values - needs manual input
                unsupported.push(format!("{}:u32 (requires manual value - see examples for defaults)", var_name));
            }
            "u256" => {
                // For preimages or generic u256, use placeholder
                unsupported.push(format!("{}:u256 (preimage/hash - requires manual input)", var_name));
            }
            "SumType" => {
                // Complex sum type - cannot auto-generate
                unsupported.push(format!("{}:SumType (complex type - requires manual definition)", var_name));
            }
            "Unknown" | _ => {
                unsupported.push(format!("{}:{} (unknown type - cannot auto-generate)", var_name, var_type));
            }
        }
    }
    
    if !unsupported.is_empty() {
        return Err(format!(
            "Cannot auto-generate: {}. Please define manually.",
            unsupported.join(", ")
        ));
    }
    
    Ok((generated, needs_multisig_selection))
}

fn get_key_name(index: usize) -> &'static str {
    match index {
        0 => "ALICE",
        1 => "BOB",
        2 => "CHARLIE",
        3 => "DAVID",
        4 => "EVE",
        5 => "FRANK",
        6 => "GRACE",
        7 => "HEIDI",
        8 => "IVAN",
        9 => "JUDY",
        _ => "KEY",
    }
}

/// Inject values into program text (as params for multisig, as witness for simple)
fn inject_witness_values(program_text: &str, witness_values: &[(String, String, String)], as_params: bool) -> String {
    if witness_values.is_empty() {
        return program_text.to_string();
    }
    
    if as_params {
        // Inject as params (for multisig - user constructs witness from these)
        let mut param_content = String::from("mod param {\n");
        // Keep existing params
        if let Some(existing_params) = extract_module_content(program_text, "param") {
            param_content.push_str(&existing_params);
        }
        // Add new signature params
        for (var_name, var_type, value) in witness_values {
            param_content.push_str(&format!("    const {}: {} = {};\n", var_name, var_type, value));
        }
        param_content.push_str("}");
        
        // Replace or insert param module
        if program_text.contains("mod param") {
            let re = regex::Regex::new(r"mod\s+param\s*\{[^}]*\}").unwrap();
            if let Some(mat) = re.find(program_text) {
                program_text.replace(mat.as_str(), &param_content)
            } else {
                program_text.to_string()
            }
        } else {
            format!("{}\n\n{}", param_content, program_text)
        }
    } else {
        // Inject as witness (for simple cases)
        let mut witness_content = String::from("mod witness {\n");
        for (var_name, var_type, value) in witness_values {
            witness_content.push_str(&format!("    const {}: {} = {};\n", var_name, var_type, value));
        }
        witness_content.push_str("}");
        
        if program_text.contains("mod witness") {
            let re = regex::Regex::new(r"mod\s+witness\s*\{[^}]*\}").unwrap();
            if let Some(mat) = re.find(program_text) {
                program_text.replace(mat.as_str(), &witness_content)
            } else {
                program_text.to_string()
            }
        } else {
            format!("{}\n\n{}", witness_content, program_text)
        }
    }
}

fn extract_module_content(program_text: &str, module_name: &str) -> Option<String> {
    let re = regex::Regex::new(&format!(r"mod\s+{}\s*\{{([^}}]*)\}}", module_name)).ok()?;
    re.captures(program_text).and_then(|cap| cap.get(1).map(|m| m.as_str().to_string()))
}

#[component]
pub fn TestnetAutomationButtons() -> impl IntoView {
    let program = use_context::<Program>().expect("program should exist in context");
    let tx_env = use_context::<TxEnv>().expect("transaction environment should exist in context");
    
    let (fund_status, set_fund_status) = create_signal(String::new());
    let (fund_loading, set_fund_loading) = create_signal(false);
    let (lookup_status, set_lookup_status) = create_signal(String::new());
    let (lookup_loading, set_lookup_loading) = create_signal(false);
    let (sign_status, set_sign_status) = create_signal(String::new());
    let (generated_signature, set_generated_signature) = create_signal(String::new());
    let (broadcast_status, set_broadcast_status) = create_signal(String::new());
    let (broadcast_loading, set_broadcast_loading) = create_signal(false);
    let (spending_txid, set_spending_txid) = create_signal(String::new());
    
    let funding_txid = create_rw_signal(String::new());
    let (current_address, set_current_address) = create_signal(String::new());
    
    // Get signing keys and signed data for sighash signature generation
    let signing_keys = use_context::<SigningKeys>().expect("signing keys should exist");
    let signed_data = use_context::<SignedData>().expect("signed data should exist");
    
    // Parse witness fields from program
    let witness_fields = create_rw_signal(Vec::<WitnessField>::new());
    
    // Update witness fields when program changes
    leptos::create_effect(move |_| {
        let program_text = program.text.with(|t| t.clone());
        // Remove existing mod witness to parse clean declarations
        let clean_text = if let Some(start) = program_text.find("mod witness {") {
            let before = &program_text[..start];
            if let Some(end_pos) = program_text[start..].find('}') {
                let after = &program_text[start + end_pos + 1..];
                format!("{}{}", before, after)
            } else {
                program_text
            }
        } else {
            program_text
        };
        
        let fields = witness_inputs::parse_witness_fields(&clean_text);
        witness_fields.set(fields);
    });
    
    // Step 1: Fund from faucet (using CORS proxy)
    let auto_fund = move |_| {
        let address = program
            .cmr()
            .ok()
            .map(util::liquid_testnet_address)
            .map(|addr| addr.to_string())
            .unwrap_or_default();

        if address.is_empty() || address == "Invalid program" {
            set_fund_status.set("Invalid program - cannot generate address".to_string());
            return;
        }

        set_current_address.set(address.clone());
        set_fund_loading.set(true);
        set_fund_status.set("Requesting funds via CORS proxy...".to_string());

        spawn_local(async move {
            match call_fund_from_faucet(&address).await {
                Ok(txid) => {
                    funding_txid.set(txid.clone());
                    set_fund_status.set(format!("✓ Funded! Txid: {}...", &txid[..16]));
                    set_fund_loading.set(false);
                }
                Err(err) => {
                    set_fund_status.set(format!("✗ {}",err));
                    set_fund_loading.set(false);
                }
            }
        });
    };
    
    // Step 3: Auto-generate signature values for Signature type fields
    let generate_signatures = move |_| {
        set_sign_status.set("Auto-generating signatures...".to_string());
        
        // Get the transaction sighash (this is the message to sign)
        let message = signed_data.message.get();
        
        // Auto-fill Signature fields
        let mut filled_any = false;
        witness_fields.update(|fields| {
            for field in fields.iter_mut() {
                if field.type_name == "Signature" || field.type_name == "[u8; 64]" {
                    // Generate signature using first key
                    let signature = signing_keys.secret_keys[0].sign_schnorr(message);
                    field.value = format!("0x{}", signature.as_ref().to_lower_hex_string());
                    filled_any = true;
                }
            }
        });
        
        if filled_any {
            set_sign_status.set("✓ Auto-generated signatures for Signature fields".to_string());
            // Show first generated signature for reference
            if let Some(field) = witness_fields.with(|fields| {
                fields.iter().find(|f| f.type_name == "Signature" && !f.value.is_empty()).cloned()
            }) {
                set_generated_signature.set(field.value);
            }
        } else {
            set_sign_status.set("⚠ No Signature fields found to auto-generate".to_string());
        }
    };

    let lookup_utxo = move |_| {
        let txid = funding_txid.get();
        let address = program
            .cmr()
            .ok()
            .map(util::liquid_testnet_address)
            .map(|addr| addr.to_string())
            .unwrap_or_default();

        if txid.is_empty() {
            set_lookup_status.set("No funding transaction. Fund first.".to_string());
            return;
        }

        if address.is_empty() {
            set_lookup_status.set("Invalid program address".to_string());
            return;
        }

        set_lookup_loading.set(true);
        set_lookup_status.set("Looking up UTXO...".to_string());

        spawn_local(async move {
            match call_lookup_utxo(&txid, &address).await {
                Ok((vout, value)) => {
                    tx_env.params.update(|params| {
                        if let Ok(parsed_txid) = txid.parse() {
                            params.txid = parsed_txid;
                        }
                        params.vout = vout;
                        params.value_in = value;
                    });
                    set_lookup_status.set(format!("Found! vout={}, value={} sats", vout, value));
                    set_lookup_loading.set(false);
                }
                Err(err) => {
                    set_lookup_status.set(format!("Error: {}", err));
                    set_lookup_loading.set(false);
                }
            }
        });
    };

    // Broadcast transaction with helpful error messages
    let broadcast_tx = move |_| {
        set_broadcast_loading.set(true);
        set_broadcast_status.set("Generating transaction...".to_string());
        
        // Inject witness module from user inputs
        let current_program_text = program.text.with(|t| t.clone());
        let fields = witness_fields.get();
        
        // Generate witness module
        let witness_module = witness_inputs::generate_witness_module(&fields);
        
        // Remove old mod witness if exists, then inject new one
        let clean_text = if let Some(start) = current_program_text.find("mod witness {") {
            let before = &current_program_text[..start];
            if let Some(end_pos) = current_program_text[start..].find('}') {
                let after = &current_program_text[start + end_pos + 1..];
                format!("{}{}", before, after)
            } else {
                current_program_text.clone()
            }
        } else {
            current_program_text.clone()
        };
        
        // Inject witness at the top
        let final_program_text = format!("{}\n{}", witness_module, clean_text);
        
        // Temporarily update program text for compilation
        let original_text = program.text.get();
        program.text.set(final_program_text);
        program.update_on_read();
        
        let params = tx_env.params;
        let env = tx_env.lazy_env;
        
        let raw_tx = with!(|params, env| {
            use elements::pset::serialize::Serialize;
            use hex_conservative::DisplayHex;
            use simplicityhl::elements;
            
            let satisfied = match program.satisfied() {
                Ok(x) => x,
                Err(e) => {
                    set_broadcast_status.set(format!("✗ Program error: {}. Fix your program first.", e));
                    set_broadcast_loading.set(false);
                    // Restore original text
                    program.text.set(original_text.clone());
                    program.update_on_read();
                    return String::new();
                }
            };
            let pruned = match satisfied.redeem().prune(env) {
                Ok(x) => x,
                Err(e) => {
                    set_broadcast_status.set(format!("✗ Transaction validation failed: {}. Check that Step 3 (signature generation) was completed and the witness data matches your program.", e));
                    set_broadcast_loading.set(false);
                    return String::new();
                }
            };
            params.transaction(&pruned).serialize().to_lower_hex_string()
        });
        
        // Restore original program text
        program.text.set(original_text);
        program.update_on_read();
        
        if raw_tx.is_empty() {
            return;
        }

        set_broadcast_status.set("Broadcasting to network...".to_string());

        spawn_local(async move {
            match call_broadcast_transaction(&raw_tx).await {
                Ok((spending_tx, explorer_url)) => {
                    // Log for debugging
                    web_sys::console::log_1(&format!("✓ Spending transaction broadcast! Txid: {}", spending_tx).into());
                    web_sys::console::log_1(&format!("Explorer URL: {}", explorer_url).into());
                    
                    set_spending_txid.set(spending_tx.clone());
                    set_broadcast_status.set(format!("✓ Spending transaction broadcast! Txid: {}", spending_tx));
                    set_broadcast_loading.set(false);
                    
                    // Open the SPENDING transaction (not the funding transaction)
                    if let Some(window) = web_sys::window() {
                        let _ = window.open_with_url_and_target(&explorer_url, "_blank");
                    }
                }
                Err(err) => {
                    set_broadcast_status.set(format!("✗ Broadcast failed: {}", err));
                    set_broadcast_loading.set(false);
                }
            }
        });
    };

    view! {
        <div class="unified-workflow">
            <h3 class="workflow-title">"Complete Transaction Workflow"</h3>
            
            <div class="workflow-step">
                <div class="step-header">
                    <span class="step-number">"1"</span>
                    <h4>"Get Testnet Funds"</h4>
                </div>
                <button
                    class="workflow-button"
                    on:click=auto_fund
                    disabled=move || fund_loading.get()
                >
                    {move || if fund_loading.get() {
                        view! { <><i class="fas fa-spinner fa-spin"></i>" Requesting..."</> }
                    } else if !fund_status.get().is_empty() && fund_status.get().contains("✓") {
                        view! { <><i class="fas fa-check-circle"></i>" Funded"</> }
                    } else {
                        view! { <><i class="fas fa-faucet"></i>" Request Testnet L-BTC"</> }
                    }}
                </button>
                {move || {
                    let addr = current_address.get();
                    let txid = funding_txid.get();
                    if !addr.is_empty() || !txid.is_empty() {
                        view! {
                            <div class="step-data">
                                {if !addr.is_empty() {
                                    view! {
                                        <>
                                            <label>"Address:"</label>
                                            <input type="text" readonly value=addr.clone() on:click=move |e| {
                                                let target = leptos::event_target::<web_sys::HtmlInputElement>(&e);
                                                target.select();
                                            } />
                                        </>
                                    }.into_view()
                                } else {
                                    view! { <span style="display:none"></span> }.into_view()
                                }}
                                {if !txid.is_empty() {
                                    let funding_url = format!("https://blockstream.info/liquidtestnet/tx/{}", txid);
                                    view! {
                                        <>
                                            <label>"Funding Txid:"</label>
                                            <input type="text" readonly value=txid.clone() on:click=move |e| {
                                                let target = leptos::event_target::<web_sys::HtmlInputElement>(&e);
                                                target.select();
                                            } />
                                            <button
                                                class="workflow-button"
                                                style="margin-top: 8px;"
                                                on:click=move |_| {
                                                    if let Some(window) = web_sys::window() {
                                                        let _ = window.open_with_url_and_target(&funding_url, "_blank");
                                                    }
                                                }
                                            >
                                                <i class="fas fa-external-link-alt"></i>
                                                " View Funding Transaction"
                                            </button>
                                        </>
                                    }.into_view()
                                } else {
                                    view! { <span style="display:none"></span> }.into_view()
                                }}
                            </div>
                        }.into_view()
                    } else {
                        view! { <span style="display:none"></span> }.into_view()
                    }
                }}
                {move || {
                    let status = fund_status.get();
                    if !status.is_empty() {
                        view! { <p class="step-status">{status}</p> }.into_view()
                    } else {
                        view! { <span style="display:none"></span> }.into_view()
                    }
                }}
            </div>

            <div class="workflow-step">
                <div class="step-header">
                    <span class="step-number">"2"</span>
                    <h4>"Lookup UTXO"</h4>
                </div>
                <button
                    class="workflow-button"
                    on:click=lookup_utxo
                    disabled=move || lookup_loading.get() || funding_txid.get().is_empty()
                >
                    {move || if lookup_loading.get() {
                        view! { <><i class="fas fa-spinner fa-spin"></i>" Looking up..."</> }
                    } else if !lookup_status.get().is_empty() && lookup_status.get().contains("✓") {
                        view! { <><i class="fas fa-check-circle"></i>" UTXO Found"</> }
                    } else {
                        view! { <><i class="fas fa-search"></i>" Find UTXO"</> }
                    }}
                </button>
                {move || {
                    let status = lookup_status.get();
                    if !status.is_empty() {
                        view! { <p class="step-status">{status}</p> }.into_view()
                    } else {
                        view! { <span style="display:none"></span> }.into_view()
                    }
                }}
            </div>

            <div class="workflow-step">
                <div class="step-header">
                    <span class="step-number">"3"</span>
                    <h4>"Witness Values"</h4>
                </div>
                
                {move || {
                    let fields = witness_fields.get();
                    if fields.is_empty() {
                        view! {
                            <p class="help-text">
                                "✓ No witness values required for this contract."
                            </p>
                        }.into_view()
                    } else {
                        view! {
                            <div class="witness-inputs-container">
                                <p class="help-text">
                                    "Enter witness values below. Leave empty to use defaults."
                                </p>
                                <For
                                    each=move || witness_fields.get()
                                    key=|field| field.name.clone()
                                    children=move |field: WitnessField| {
                                        let field_name = field.name.clone();
                                        
                                        view! {
                                            <div class="witness-field">
                                                <label>
                                                    <span class="field-name">{field.name.clone()}</span>
                                                    <span class="field-type">" ("{field.type_name.clone()}")"</span>
                                                </label>
                                                <input
                                                    type="text"
                                                    placeholder=field.placeholder.clone()
                                                    value=field.value.clone()
                                                    on:input=move |e| {
                                                        let new_value = leptos::event_target_value(&e);
                                                        witness_fields.update(|fields| {
                                                            if let Some(f) = fields.iter_mut().find(|f| f.name == field_name) {
                                                                f.value = new_value;
                                                            }
                                                        });
                                                    }
                                                />
                                            </div>
                                        }
                                    }
                                />
                                
                                <button
                                    class="workflow-button"
                                    style="margin-top: 12px;"
                                    on:click=generate_signatures
                                    disabled=move || !lookup_status.get().contains("Found") && !lookup_status.get().contains("Auto-filled")
                                >
                                    <i class="fas fa-key"></i>
                                    " Auto-Generate Signatures"
                                </button>
                            </div>
                        }.into_view()
                    }
                }}
                {move || {
                    let sig = generated_signature.get();
                    if !sig.is_empty() {
                        view! {
                            <div class="step-data">
                                <label>"Primary Signature:"</label>
                                <input
                                    type="text"
                                    readonly
                                    value=sig.clone()
                                    on:click=move |e| {
                                        let target = leptos::event_target::<web_sys::HtmlInputElement>(&e);
                                        target.select();
                                        if let Some(window) = web_sys::window() {
                                            let _ = window.navigator().clipboard().write_text(&sig);
                                        }
                                    }
                                />
                                <p class="hint">"✓ All witness values auto-injected into program (click to copy)"</p>
                            </div>
                        }.into_view()
                    } else {
                        view! { <span style="display:none"></span> }.into_view()
                    }
                }}
                {move || {
                    let status = sign_status.get();
                    if !status.is_empty() {
                        view! { <p class="step-status">{status}</p> }.into_view()
                    } else {
                        view! { <span style="display:none"></span> }.into_view()
                    }
                }}
            </div>

            <div class="workflow-step">
                <div class="step-header">
                    <span class="step-number">"4"</span>
                    <h4>"Broadcast Transaction"</h4>
                </div>
                <button
                    class="workflow-button primary"
                    on:click=broadcast_tx
                    disabled=move || broadcast_loading.get()
                >
                    {move || if broadcast_loading.get() {
                        view! { <><i class="fas fa-spinner fa-spin"></i>" Broadcasting..."</> }
                    } else if !broadcast_status.get().is_empty() && broadcast_status.get().contains("✓") {
                        view! { <><i class="fas fa-check-circle"></i>" Transaction Sent!"</> }
                    } else {
                        view! { <><i class="fas fa-rocket"></i>" Broadcast to Network"</> }
                    }}
                </button>
                {move || {
                    let status = broadcast_status.get();
                    if !status.is_empty() {
                        view! { <p class="step-status">{status}</p> }.into_view()
                    } else {
                        view! { <span style="display:none"></span> }.into_view()
                    }
                }}
                {move || {
                    let txid = spending_txid.get();
                    if !txid.is_empty() {
                        let explorer_url = format!("https://blockstream.info/liquidtestnet/tx/{}", txid);
                        view! {
                            <div class="step-data">
                                <label>"Spending Transaction:"</label>
                                <input
                                    type="text"
                                    readonly
                                    value=txid.clone()
                                    on:click=move |e| {
                                        let target = leptos::event_target::<web_sys::HtmlInputElement>(&e);
                                        target.select();
                                    }
                                />
                                <button
                                    class="workflow-button"
                                    style="margin-top: 10px;"
                                    on:click=move |_| {
                                        if let Some(window) = web_sys::window() {
                                            let _ = window.open_with_url_and_target(&explorer_url, "_blank");
                                        }
                                    }
                                >
                                    <i class="fas fa-external-link-alt"></i>
                                    " View Spending Transaction"
                                </button>
                            </div>
                        }.into_view()
                    } else {
                        view! { <span style="display:none"></span> }.into_view()
                    }
                }}
            </div>
        </div>
    }
}

async fn call_fund_from_faucet(address: &str) -> Result<String, String> {
    let window = web_sys::window().ok_or("No window")?;
    let automation_class = js_sys::Reflect::get(&window, &"TestnetAutomation".into())
        .map_err(|_| "TestnetAutomation not found")?;
    
    let automation = js_sys::Reflect::construct(&automation_class.into(), &js_sys::Array::new())
        .map_err(|_| "Failed to create TestnetAutomation instance")?;
    
    let fund_fn = js_sys::Reflect::get(&automation, &"fundFromFaucet".into())
        .map_err(|_| "fundFromFaucet method not found")?;
    
    let promise = js_sys::Reflect::apply(
        &fund_fn.into(),
        &automation,
        &js_sys::Array::of1(&address.into()),
    )
    .map_err(|_| "Failed to call fundFromFaucet")?;
    
    let result = wasm_bindgen_futures::JsFuture::from(js_sys::Promise::from(promise))
        .await
        .map_err(|e| format!("{:?}", e))?;
    
    let txid = js_sys::Reflect::get(&result, &"txid".into())
        .map_err(|_| "No txid in result")?
        .as_string()
        .ok_or("txid is not a string")?;
    
    Ok(txid)
}

async fn call_lookup_utxo(txid: &str, address: &str) -> Result<(u32, u64), String> {
    let window = web_sys::window().ok_or("No window")?;
    let automation_class = js_sys::Reflect::get(&window, &"TestnetAutomation".into())
        .map_err(|_| "TestnetAutomation not found")?;
    
    let automation = js_sys::Reflect::construct(&automation_class.into(), &js_sys::Array::new())
        .map_err(|_| "Failed to create TestnetAutomation instance")?;
    
    let lookup_fn = js_sys::Reflect::get(&automation, &"lookupUTXO".into())
        .map_err(|_| "lookupUTXO method not found")?;
    
    let args = js_sys::Array::new();
    args.push(&txid.into());
    args.push(&address.into());
    
    let promise = js_sys::Reflect::apply(&lookup_fn.into(), &automation, &args)
        .map_err(|_| "Failed to call lookupUTXO")?;
    
    let result = wasm_bindgen_futures::JsFuture::from(js_sys::Promise::from(promise))
        .await
        .map_err(|e| format!("{:?}", e))?;
    
    let vout = js_sys::Reflect::get(&result, &"vout".into())
        .map_err(|_| "No vout in result")?
        .as_f64()
        .ok_or("vout is not a number")? as u32;
    
    let value = js_sys::Reflect::get(&result, &"value".into())
        .map_err(|_| "No value in result")?
        .as_f64()
        .ok_or("value is not a number")? as u64;
    
    Ok((vout, value))
}

async fn call_broadcast_transaction(raw_tx: &str) -> Result<(String, String), String> {
    let window = web_sys::window().ok_or("No window")?;
    let automation_class = js_sys::Reflect::get(&window, &"TestnetAutomation".into())
        .map_err(|_| "TestnetAutomation not found")?;
    
    let automation = js_sys::Reflect::construct(&automation_class.into(), &js_sys::Array::new())
        .map_err(|_| "Failed to create TestnetAutomation instance")?;
    
    let broadcast_fn = js_sys::Reflect::get(&automation, &"broadcastTransaction".into())
        .map_err(|_| "broadcastTransaction method not found")?;
    
    let promise = js_sys::Reflect::apply(
        &broadcast_fn.into(),
        &automation,
        &js_sys::Array::of1(&raw_tx.into()),
    )
    .map_err(|_| "Failed to call broadcastTransaction")?;
    
    let result = wasm_bindgen_futures::JsFuture::from(js_sys::Promise::from(promise))
        .await
        .map_err(|e| format!("{:?}", e))?;
    
    let txid = js_sys::Reflect::get(&result, &"txid".into())
        .map_err(|_| "No txid in result")?
        .as_string()
        .ok_or("txid is not a string")?;
    
    let explorer_url = js_sys::Reflect::get(&result, &"explorerUrl".into())
        .map_err(|_| "No explorerUrl in result")?
        .as_string()
        .ok_or("explorerUrl is not a string")?;
    
    Ok((txid, explorer_url))
}
