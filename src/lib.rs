use std::{fs, path::PathBuf};

use boa_engine::{Context, Source};
use lazy_regex::regex;
use serde::{Deserialize, Serialize};
use swc_core::{
    ecma::ast::Program,
    plugin::{errors::HANDLER, plugin_transform, proxies::TransformPluginProgramMetadata},
};

static JS_VISITOR_IMPORT_REGEX: &lazy_regex::Lazy<lazy_regex::Regex> =
    regex!(r#"import(?:([\w*{Visitor}\n\r\t, ]+)[\s*]from)?[\s*](?:["']@swc\/core\/Visitor["'])?"#);
const JS_VISITOR_STR: &str = include_str!("../node_modules/@swc/core/Visitor.js");

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TransformPluginConfig {
    pub transform_impl_path: Option<String>,
    pub visitor_class_name: Option<String>,
}

impl Default for TransformPluginConfig {
    fn default() -> Self {
        Self {
            transform_impl_path: None,
            visitor_class_name: None,
        }
    }
}

struct TransformContext {
    pub transform_impl: String,
    pub transform_visitor_class_name: String,
}

#[plugin_transform]
pub fn process(program: Program, metadata: TransformPluginProgramMetadata) -> Program {
    if let Some(transform_context) =
        build_transform_context(&metadata.get_transform_plugin_config())
    {
        return HANDLER.with(|handler| {
            // Serialize the AST into JSON to pass into JS context
            let serde_serialized_ast = serde_json::to_string(&program);
            if let Err(err) = serde_serialized_ast {
                handler.err(
                    format!(
                        "Failed to serialize AST into JSON, cannot perform transform {:#?}",
                        err
                    )
                    .as_str(),
                );

                return program;
            }

            let serde_serialized_ast = serde_serialized_ast.unwrap();

            // Create the JavaScript context.
            let mut context = Context::default();

            // Set serialized ast into global object.
            let set_ast_result =
                context
                    .global_object()
                    .set("ast", serde_serialized_ast, true, &mut context);
            if let Err(err) = set_ast_result {
                handler.err(
                    format!(
                        "Failed to set AST into JS context, cannot perform transform {:#?}",
                        err
                    )
                    .as_str(),
                );

                return program;
            }

            // Run the actual transform.

            // Build base visitor class sources.
            // Manually removing exports from the cjs module as default context does not understand it.
            let visitor_str = JS_VISITOR_STR.lines().filter(|line| {
                !line.starts_with("exports.")
                    && !line.starts_with(
                        r#"Object.defineProperty(exports, "__esModule", { value: true });"#,
                    )
            });

            // Build custom transform visitor inherits above visitor class, actual transformer
            // Manually removes import to the named visitor class as we inject class automatically & we don't need to
            // resolve to the external module.
            let transform_impl_content = transform_context
                .transform_impl
                .lines()
                .filter(|line| !JS_VISITOR_IMPORT_REGEX.is_match(line));

            let mut transform_codes = visitor_str
                .chain(transform_impl_content)
                .collect::<Vec<&str>>();

            // Finally, append the actual code to perform transform.
            let code = format!(
                "JSON.stringify((new {}()).visitProgram(JSON.parse(ast)))",
                transform_context.transform_visitor_class_name
            );
            transform_codes.push(code.as_str());

            let transform_code = transform_codes.join("\n");

            let transform_result = context.eval(Source::from_bytes(transform_code.as_str()));
            if let Err(err) = transform_result {
                handler.err(
                    format!(
                        "Failed to run transform, cannot perform transform {:#?}",
                        err
                    )
                    .as_str(),
                );

                return program;
            }

            let transform_result = transform_result
                .unwrap()
                .as_string()
                .unwrap()
                .to_std_string_escaped();

            let transformed_program = serde_json::from_str::<Program>(transform_result.as_str());

            if let Err(err) = transformed_program {
                handler.err(
                    format!(
                        "Failed to deserialize transformed AST, cannot perform transform {:#?}",
                        err
                    )
                    .as_str(),
                );

                return program;
            }

            let transformed_program = transformed_program.unwrap();
            return transformed_program;
        });
    } else {
        return program;
    }
}

fn build_transform_context(config_str: &Option<String>) -> Option<TransformContext> {
    HANDLER.with(|handler| {
        if config_str.is_none() {
            handler.err("Plugin config object is not supplied, skipping transform");
            return None;
        }

        let deserialized_config =
            serde_json::from_str::<TransformPluginConfig>(&config_str.as_ref().unwrap());
        if deserialized_config.is_err() {
            let err = deserialized_config.err().unwrap();
            handler.err(
                format!(
                    "Failed to deserialize plugin config object, skipping transform {:#?}",
                    err
                )
                .as_str(),
            );
            return None;
        }

        let deserialized_config = deserialized_config.unwrap();
        let transform_impl_content =
            if let Some(transform_impl_path) = &deserialized_config.transform_impl_path {
                let mut p = PathBuf::from("/cwd");
                p.push(transform_impl_path);
                let content_result = fs::read_to_string(p);
                match content_result {
                    Ok(content) => content,
                    Err(err) => {
                        handler.err(
                            format!(
                                "Failed to read transform impl from path, skipping transform {:#?}",
                                err
                            )
                            .as_str(),
                        );
                        return None;
                    }
                }
            } else {
                handler.err("Transform impl path is not supplied, skipping transform");
                return None;
            };

        let transform_visitor_class_name = deserialized_config
            .visitor_class_name
            .unwrap_or("TransformVisitor".to_string());

        return Some(TransformContext {
            transform_impl: transform_impl_content,
            transform_visitor_class_name,
        });
    })
}
