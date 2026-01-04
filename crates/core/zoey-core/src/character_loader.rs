//! Character configuration loader from XML files

use crate::types::*;
use crate::Result;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Load character from XML file
pub fn load_character_from_xml(path: impl AsRef<Path>) -> Result<Character> {
    let xml_content = fs::read_to_string(path)
        .map_err(|e| crate::ZoeyError::config(format!("Failed to read character file: {}", e)))?;

    parse_character_xml(&xml_content)
}

/// Parse character from XML string
pub fn parse_character_xml(xml: &str) -> Result<Character> {
    // Simple XML parsing (in production, would use xml-rs or quick-xml crate)
    let mut character = Character::default();

    // Extract name
    if let Some(name) = extract_tag_content(xml, "name") {
        character.name = name;
    }

    // Extract username
    if let Some(username) = extract_tag_content(xml, "username") {
        character.username = Some(username);
    }

    // Extract bio entries
    character.bio = extract_multiple_tags(xml, "bio", "entry");

    // Extract lore entries
    character.lore = extract_multiple_tags(xml, "lore", "entry");

    // Extract knowledge entries
    character.knowledge = extract_multiple_tags(xml, "knowledge", "entry");

    // Extract adjectives
    character.adjectives = extract_multiple_tags(xml, "adjectives", "adjective");

    // Extract topics
    character.topics = extract_multiple_tags(xml, "topics", "topic");

    // Extract style
    character.style = CharacterStyle {
        all: extract_multiple_tags(xml, "style/all", "guideline"),
        chat: extract_multiple_tags(xml, "style/chat", "guideline"),
        post: extract_multiple_tags(xml, "style/post", "guideline"),
    };

    // Extract post examples
    character.post_examples = extract_multiple_tags(xml, "postExamples", "post");

    // Extract message examples (conversations)
    character.message_examples = extract_message_examples(xml);

    // Extract plugins
    character.plugins = extract_multiple_tags(xml, "plugins", "plugin");

    // Extract clients
    character.clients = extract_multiple_tags(xml, "clients", "client");

    // Extract templates
    if let Some(templates_section) = extract_section(xml, "templates") {
        let mut templates = CharacterTemplates {
            message_handler_template: None,
            post_creation_template: None,
            custom: HashMap::new(),
        };

        if let Some(msg_handler) = extract_cdata_content(&templates_section, "messageHandler") {
            templates.message_handler_template = Some(msg_handler);
        }

        if let Some(post_creation) = extract_cdata_content(&templates_section, "postCreation") {
            templates.post_creation_template = Some(post_creation);
        }

        if let Some(should_respond) = extract_cdata_content(&templates_section, "shouldRespond") {
            templates
                .custom
                .insert("shouldRespond".to_string(), should_respond);
        }

        character.templates = Some(templates);
    }

    // Extract settings
    let mut settings = HashMap::new();
    if let Some(settings_section) = extract_section(xml, "settings") {
        // Model provider
        if let Some(model_provider) = extract_tag_content(&settings_section, "model_provider") {
            settings.insert(
                "model_provider".to_string(),
                serde_json::json!(model_provider),
            );
            settings.insert(
                "MODEL_PROVIDER".to_string(),
                serde_json::json!(model_provider),
            );
            character.model_provider = Some(model_provider.clone());
        }

        // OpenAI settings
        if let Some(openai_model) = extract_tag_content(&settings_section, "openai_model") {
            settings.insert("OPENAI_MODEL".to_string(), serde_json::json!(openai_model));
            settings.insert("openai_model".to_string(), serde_json::json!(openai_model));
        }
        if let Some(openai_key) = extract_tag_content(&settings_section, "openai_api_key") {
            if !openai_key.is_empty() {
                settings.insert("OPENAI_API_KEY".to_string(), serde_json::json!(openai_key));
            }
        }

        // Anthropic settings
        if let Some(anthropic_model) = extract_tag_content(&settings_section, "anthropic_model") {
            settings.insert(
                "ANTHROPIC_MODEL".to_string(),
                serde_json::json!(anthropic_model),
            );
        }
        if let Some(anthropic_key) = extract_tag_content(&settings_section, "anthropic_api_key") {
            if !anthropic_key.is_empty() {
                settings.insert(
                    "ANTHROPIC_API_KEY".to_string(),
                    serde_json::json!(anthropic_key),
                );
            }
        }

        // Local LLM settings
        if let Some(local_model) = extract_tag_content(&settings_section, "local_llm_model") {
            settings.insert(
                "LOCAL_LLM_MODEL".to_string(),
                serde_json::json!(local_model),
            );
            settings.insert(
                "local_llm_model".to_string(),
                serde_json::json!(local_model),
            );
        }
        if let Some(endpoint) = extract_tag_content(&settings_section, "local_llm_endpoint") {
            settings.insert(
                "LOCAL_LLM_ENDPOINT".to_string(),
                serde_json::json!(endpoint),
            );
        }

        // Fallback provider
        if let Some(fallback) = extract_tag_content(&settings_section, "fallback_provider") {
            settings.insert("fallback_provider".to_string(), serde_json::json!(fallback));
        }

        // General settings
        if let Some(temp) = extract_tag_content(&settings_section, "temperature") {
            if let Ok(temp_f) = temp.parse::<f32>() {
                settings.insert("temperature".to_string(), serde_json::json!(temp_f));
            }
        }
        if let Some(max_tokens) = extract_tag_content(&settings_section, "max_tokens") {
            if let Ok(tokens) = max_tokens.parse::<usize>() {
                settings.insert("max_tokens".to_string(), serde_json::json!(tokens));
            }
        }

        // Training settings
        if let Some(training_enabled) = extract_tag_content(&settings_section, "training_enabled") {
            if let Ok(enabled) = training_enabled.parse::<bool>() {
                settings.insert("training_enabled".to_string(), serde_json::json!(enabled));
            }
        }
        if let Some(min_quality) = extract_tag_content(&settings_section, "training_min_quality") {
            if let Ok(quality) = min_quality.parse::<f32>() {
                settings.insert(
                    "training_min_quality".to_string(),
                    serde_json::json!(quality),
                );
            }
        }
        if let Some(rlhf) = extract_tag_content(&settings_section, "rlhf_enabled") {
            if let Ok(enabled) = rlhf.parse::<bool>() {
                settings.insert("rlhf_enabled".to_string(), serde_json::json!(enabled));
            }
        }
        
        // Training backend settings (for MCP auto-configuration)
        if let Some(training_backend) = extract_tag_content(&settings_section, "training_backend") {
            settings.insert("training_backend".to_string(), serde_json::json!(training_backend));
        }
        if let Some(base_model) = extract_tag_content(&settings_section, "training_base_model") {
            settings.insert("training_base_model".to_string(), serde_json::json!(base_model));
        }
        if let Some(method) = extract_tag_content(&settings_section, "training_method") {
            settings.insert("training_method".to_string(), serde_json::json!(method));
        }
        if let Some(use_gpu) = extract_tag_content(&settings_section, "training_use_gpu") {
            if let Ok(gpu) = use_gpu.parse::<bool>() {
                settings.insert("training_use_gpu".to_string(), serde_json::json!(gpu));
            }
        }
        if let Some(output_dir) = extract_tag_content(&settings_section, "training_output_dir") {
            settings.insert("training_output_dir".to_string(), serde_json::json!(output_dir));
        }
        if let Some(num_epochs) = extract_tag_content(&settings_section, "training_num_epochs") {
            if let Ok(epochs) = num_epochs.parse::<u32>() {
                settings.insert("training_num_epochs".to_string(), serde_json::json!(epochs));
            }
        }
        if let Some(lora_rank) = extract_tag_content(&settings_section, "training_lora_rank") {
            if let Ok(rank) = lora_rank.parse::<u32>() {
                settings.insert("training_lora_rank".to_string(), serde_json::json!(rank));
            }
        }

        // Dynamic prompt settings
        if let Some(max_entries) =
            extract_tag_content(&settings_section, "dynamic_prompt_max_entries")
        {
            if let Ok(entries) = max_entries.parse::<usize>() {
                settings.insert(
                    "dynamic_prompt_max_entries".to_string(),
                    serde_json::json!(entries),
                );
            }
        }
        if let Some(validation) = extract_tag_content(&settings_section, "validation_level") {
            settings.insert(
                "validation_level".to_string(),
                serde_json::json!(validation),
            );
        }

        // Cache settings
        if let Some(cache_ttl) = extract_tag_content(&settings_section, "entity_cache_ttl") {
            if let Ok(ttl) = cache_ttl.parse::<u64>() {
                settings.insert("entity_cache_ttl".to_string(), serde_json::json!(ttl));
            }
        }

        // Performance settings
        if let Some(conv_len) = extract_tag_content(&settings_section, "conversation_length") {
            if let Ok(len) = conv_len.parse::<usize>() {
                settings.insert("conversation_length".to_string(), serde_json::json!(len));
            }
        }
        if let Some(retries) = extract_tag_content(&settings_section, "max_retries") {
            if let Ok(r) = retries.parse::<usize>() {
                settings.insert("max_retries".to_string(), serde_json::json!(r));
            }
        }
    }

    // Extract voice configuration as a nested object
    if let Some(voice_section) = extract_section(xml, "voice") {
        let mut voice_config = serde_json::Map::new();

        // Core settings
        if let Some(enabled) = extract_tag_content(&voice_section, "enabled") {
            voice_config.insert("enabled".to_string(), serde_json::json!(enabled == "true"));
        }
        if let Some(engine) = extract_tag_content(&voice_section, "engine") {
            voice_config.insert("engine".to_string(), serde_json::json!(engine));
        }
        if let Some(model) = extract_tag_content(&voice_section, "model") {
            voice_config.insert("model".to_string(), serde_json::json!(model));
        }
        if let Some(voice_id) = extract_tag_content(&voice_section, "voice_id") {
            voice_config.insert("voice_id".to_string(), serde_json::json!(voice_id));
        }
        if let Some(voice_name) = extract_tag_content(&voice_section, "voice_name") {
            voice_config.insert("voice_name".to_string(), serde_json::json!(voice_name));
        }
        if let Some(speed) = extract_tag_content(&voice_section, "speed") {
            if let Ok(s) = speed.parse::<f32>() {
                voice_config.insert("speed".to_string(), serde_json::json!(s));
            }
        }
        if let Some(output_format) = extract_tag_content(&voice_section, "output_format") {
            voice_config.insert(
                "output_format".to_string(),
                serde_json::json!(output_format),
            );
        }
        if let Some(sample_rate) = extract_tag_content(&voice_section, "sample_rate") {
            if let Ok(sr) = sample_rate.parse::<u32>() {
                voice_config.insert("sample_rate".to_string(), serde_json::json!(sr));
            }
        }
        if let Some(streaming) = extract_tag_content(&voice_section, "streaming") {
            voice_config.insert(
                "streaming".to_string(),
                serde_json::json!(streaming == "true"),
            );
        }

        // Triggers
        if let Some(triggers_section) = extract_section(&voice_section, "triggers") {
            let mut triggers = Vec::new();
            let mut pos = 0;
            while let Some(start) = triggers_section[pos..].find("<trigger>") {
                let content_start = pos + start + "<trigger>".len();
                if let Some(end) = triggers_section[content_start..].find("</trigger>") {
                    let trigger = triggers_section[content_start..content_start + end].trim();
                    if !trigger.is_empty() {
                        triggers.push(serde_json::json!(trigger));
                    }
                    pos = content_start + end + "</trigger>".len();
                } else {
                    break;
                }
            }
            if !triggers.is_empty() {
                voice_config.insert("triggers".to_string(), serde_json::Value::Array(triggers));
            }
        }

        // Discord-specific settings
        if let Some(discord_section) = extract_section(&voice_section, "discord") {
            let mut discord = serde_json::Map::new();
            if let Some(auto_join) = extract_tag_content(&discord_section, "auto_join_voice") {
                discord.insert(
                    "auto_join_voice".to_string(),
                    serde_json::json!(auto_join == "true"),
                );
            }
            if let Some(leave_alone) = extract_tag_content(&discord_section, "leave_when_alone") {
                discord.insert(
                    "leave_when_alone".to_string(),
                    serde_json::json!(leave_alone == "true"),
                );
            }
            if let Some(timeout) = extract_tag_content(&discord_section, "idle_timeout_seconds") {
                if let Ok(t) = timeout.parse::<u64>() {
                    discord.insert("idle_timeout_seconds".to_string(), serde_json::json!(t));
                }
            }
            if let Some(speak) = extract_tag_content(&discord_section, "speak_responses") {
                discord.insert(
                    "speak_responses".to_string(),
                    serde_json::json!(speak == "true"),
                );
            }
            if let Some(listen) = extract_tag_content(&discord_section, "listen_enabled") {
                discord.insert(
                    "listen_enabled".to_string(),
                    serde_json::json!(listen == "true"),
                );
            }
            voice_config.insert("discord".to_string(), serde_json::Value::Object(discord));
        }

        settings.insert("voice".to_string(), serde_json::Value::Object(voice_config));
    }

    character.settings = settings;

    Ok(character)
}

/// Extract message examples (conversations)
fn extract_message_examples(xml: &str) -> Vec<Vec<MessageExample>> {
    let mut conversations = Vec::new();

    if let Some(section) = extract_section(xml, "messageExamples") {
        // Extract all conversation blocks
        let mut search_pos = 0;
        while let Some(start) = section[search_pos..].find("<conversation>") {
            let actual_start = search_pos + start;
            if let Some(end) = section[actual_start..].find("</conversation>") {
                let content_start = actual_start + "<conversation>".len();
                let content_end = actual_start + end;

                if content_start < content_end {
                    let conv_section = &section[content_start..content_end];

                    // Extract messages in this conversation
                    let mut messages = Vec::new();
                    let mut msg_pos = 0;

                    while let Some(msg_start) = conv_section[msg_pos..].find("<message ") {
                        let msg_actual_start = msg_pos + msg_start;
                        if let Some(msg_end) = conv_section[msg_actual_start..].find("/>") {
                            let msg_tag =
                                &conv_section[msg_actual_start..msg_actual_start + msg_end + 2];

                            // Parse name and text attributes
                            if let (Some(name), Some(text)) = (
                                extract_attribute(msg_tag, "name"),
                                extract_attribute(msg_tag, "text"),
                            ) {
                                messages.push(MessageExample { name, text });
                            }

                            msg_pos = msg_actual_start + msg_end + 2;
                        } else {
                            break;
                        }
                    }

                    if !messages.is_empty() {
                        conversations.push(messages);
                    }
                }

                search_pos = content_end + "</conversation>".len();
            } else {
                break;
            }
        }
    }

    conversations
}

/// Extract attribute value from XML tag
fn extract_attribute(tag: &str, attr: &str) -> Option<String> {
    let pattern = format!(r#"{}=""#, attr);
    if let Some(start) = tag.find(&pattern) {
        let value_start = start + pattern.len();
        if let Some(quote_end) = tag[value_start..].find('"') {
            return Some(tag[value_start..value_start + quote_end].to_string());
        }
    }
    None
}

/// Extract CDATA content from XML tag
fn extract_cdata_content(xml: &str, tag: &str) -> Option<String> {
    let start_tag = format!("<{}>", tag);
    let end_tag = format!("</{}>", tag);

    if let Some(start) = xml.find(&start_tag) {
        if let Some(end) = xml.find(&end_tag) {
            let content_start = start + start_tag.len();
            if content_start < end {
                let content = xml[content_start..end].trim();

                // Check for CDATA
                if content.starts_with("<![CDATA[") && content.ends_with("]]>") {
                    return Some(content[9..content.len() - 3].trim().to_string());
                }

                // Return regular content
                return Some(content.to_string());
            }
        }
    }

    None
}

/// Extract content from a simple XML tag
fn extract_tag_content(xml: &str, tag: &str) -> Option<String> {
    let start_tag = format!("<{}>", tag);
    let end_tag = format!("</{}>", tag);

    if let Some(start) = xml.find(&start_tag) {
        if let Some(end) = xml.find(&end_tag) {
            let content_start = start + start_tag.len();
            if content_start < end {
                return Some(xml[content_start..end].trim().to_string());
            }
        }
    }

    None
}

/// Extract multiple entries from a parent tag
fn extract_multiple_tags(xml: &str, parent_tag: &str, child_tag: &str) -> Vec<String> {
    let mut results = Vec::new();

    // Extract parent section first
    if let Some(section) = extract_section(xml, parent_tag) {
        let start_tag = format!("<{}>", child_tag);
        let end_tag = format!("</{}>", child_tag);

        let mut search_pos = 0;
        while let Some(start) = section[search_pos..].find(&start_tag) {
            let actual_start = search_pos + start;
            if let Some(end) = section[actual_start..].find(&end_tag) {
                let content_start = actual_start + start_tag.len();
                let content_end = actual_start + end;

                if content_start < content_end {
                    results.push(section[content_start..content_end].trim().to_string());
                }

                search_pos = content_end + end_tag.len();
            } else {
                break;
            }
        }
    }

    results
}

/// Extract a section between opening and closing tags
fn extract_section(xml: &str, tag: &str) -> Option<String> {
    // Handle nested tags like "style/all"
    let parts: Vec<&str> = tag.split('/').collect();

    if parts.len() == 1 {
        let start_tag = format!("<{}>", tag);
        let end_tag = format!("</{}>", tag);

        if let Some(start) = xml.find(&start_tag) {
            if let Some(end) = xml.find(&end_tag) {
                let content_start = start + start_tag.len();
                if content_start < end {
                    return Some(xml[content_start..end].to_string());
                }
            }
        }
    } else {
        // Handle nested tags
        let mut current_section = xml.to_string();
        for part in parts {
            if let Some(section) = extract_section(&current_section, part) {
                current_section = section;
            } else {
                return None;
            }
        }
        return Some(current_section);
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_tag_content() {
        let xml = "<name>TestBot</name>";
        assert_eq!(
            extract_tag_content(xml, "name"),
            Some("TestBot".to_string())
        );
    }

    #[test]
    fn test_extract_multiple_tags() {
        let xml = r#"<bio><entry>First</entry><entry>Second</entry></bio>"#;
        let entries = extract_multiple_tags(xml, "bio", "entry");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0], "First");
        assert_eq!(entries[1], "Second");
    }

    #[test]
    fn test_parse_character() {
        let xml = r#"
        <character>
            <name>TestBot</name>
            <username>testbot</username>
            <bio>
                <entry>I am a test bot</entry>
            </bio>
        </character>
        "#;

        let character = parse_character_xml(xml).unwrap();
        assert_eq!(character.name, "TestBot");
        assert_eq!(character.username, Some("testbot".to_string()));
        assert_eq!(character.bio.len(), 1);
    }

    #[test]
    fn test_parse_complete_character() {
        let xml = r#"
        <character>
            <name>TestBot</name>
            <username>testbot</username>
            <bio>
                <entry>First bio</entry>
                <entry>Second bio</entry>
            </bio>
            <lore>
                <entry>Origin story</entry>
            </lore>
            <knowledge>
                <entry>Fact 1</entry>
                <entry>Fact 2</entry>
            </knowledge>
            <postExamples>
                <post>Example post 1</post>
                <post>Example post 2</post>
            </postExamples>
            <messageExamples>
                <conversation>
                    <message name="User" text="Hello" />
                    <message name="Bot" text="Hi there!" />
                </conversation>
            </messageExamples>
            <topics>
                <topic>AI</topic>
                <topic>Rust</topic>
            </topics>
            <adjectives>
                <adjective>helpful</adjective>
                <adjective>smart</adjective>
            </adjectives>
            <style>
                <all>
                    <guideline>Be helpful</guideline>
                </all>
                <chat>
                    <guideline>Be friendly</guideline>
                </chat>
                <post>
                    <guideline>Be concise</guideline>
                </post>
            </style>
            <plugins>
                <plugin>plugin1</plugin>
                <plugin>plugin2</plugin>
            </plugins>
            <clients>
                <client>terminal</client>
            </clients>
            <templates>
                <messageHandler><![CDATA[Template content here]]></messageHandler>
                <postCreation><![CDATA[Post template]]></postCreation>
            </templates>
            <settings>
                <model_provider>openai</model_provider>
                <temperature>0.7</temperature>
                <max_tokens>1000</max_tokens>
            </settings>
        </character>
        "#;

        let character = parse_character_xml(xml).unwrap();

        // Verify all fields
        assert_eq!(character.name, "TestBot");
        assert_eq!(character.username, Some("testbot".to_string()));
        assert_eq!(character.bio.len(), 2);
        assert_eq!(character.lore.len(), 1);
        assert_eq!(character.knowledge.len(), 2);
        assert_eq!(character.post_examples.len(), 2);
        assert_eq!(character.message_examples.len(), 1);
        assert_eq!(character.message_examples[0].len(), 2);
        assert_eq!(character.topics.len(), 2);
        assert_eq!(character.adjectives.len(), 2);
        assert_eq!(character.plugins.len(), 2);
        assert_eq!(character.clients.len(), 1);
        assert_eq!(character.style.all.len(), 1);
        assert_eq!(character.style.chat.len(), 1);
        assert_eq!(character.style.post.len(), 1);
        assert!(character.templates.is_some());
        assert_eq!(character.model_provider, Some("openai".to_string()));
        assert!(character.settings.contains_key("temperature"));
    }

    #[test]
    fn test_extract_attribute() {
        let tag = r#"<message name="User" text="Hello world" />"#;
        assert_eq!(extract_attribute(tag, "name"), Some("User".to_string()));
        assert_eq!(
            extract_attribute(tag, "text"),
            Some("Hello world".to_string())
        );
        assert_eq!(extract_attribute(tag, "missing"), None);
    }

    #[test]
    fn test_extract_cdata() {
        let xml = "<template><![CDATA[Content here]]></template>";
        let content = extract_cdata_content(xml, "template");
        assert_eq!(content, Some("Content here".to_string()));
    }
}
