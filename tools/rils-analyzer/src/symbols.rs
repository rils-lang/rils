use super::*;

impl Server {
    pub(super) fn inlay_hints(&self, params: &Value) -> Result<Value, AnyError> {
        let (_, document) = self.document(params)?;
        let Some(analysis) = analysis(document) else {
            return Ok(json!([]));
        };
        let start = params
            .pointer("/range/start")
            .map(|position| {
                offset(
                    &document.text,
                    position.get("line").and_then(Value::as_u64).unwrap_or(0) as u32,
                    position
                        .get("character")
                        .and_then(Value::as_u64)
                        .unwrap_or(0) as u32,
                )
            })
            .unwrap_or(0);
        let end = params
            .pointer("/range/end")
            .map(|position| {
                offset(
                    &document.text,
                    position
                        .get("line")
                        .and_then(Value::as_u64)
                        .unwrap_or(u64::MAX) as u32,
                    position
                        .get("character")
                        .and_then(Value::as_u64)
                        .unwrap_or(u64::MAX) as u32,
                )
            })
            .unwrap_or(document.text.len());
        let hints = analysis
            .inlay_hints
            .iter()
            .filter(|hint| start <= hint.position && hint.position <= end)
            .map(|hint| {
                json!({
                    "position": {
                        "line": position(&document.text, hint.position)[0],
                        "character": position(&document.text, hint.position)[1]
                    },
                    "label": hint.label,
                    "kind": 1,
                    "tooltip": format!("Inferred type for `{}`", text_at(&document.text, hint.span))
                })
            })
            .collect::<Vec<_>>();
        Ok(json!(hints))
    }

    pub(super) fn document_symbols(&self, params: &Value) -> Result<Value, AnyError> {
        let (_, document) = self.document(params)?;
        let Some(analysis) = analysis(document) else {
            return Ok(json!([]));
        };
        let symbols = analysis
            .symbols
            .iter()
            .filter(|symbol| symbol.is_definition)
            .map(|symbol| {
                json!({
                    "name": symbol.name,
                    "kind": document_symbol_kind(symbol.kind),
                    "range": range(&document.text, symbol.span),
                    "selectionRange": range(&document.text, symbol.span)
                })
            })
            .collect::<Vec<_>>();
        Ok(json!(symbols))
    }

    pub(super) fn semantic_tokens(&self, params: &Value) -> Result<Value, AnyError> {
        let (_, document) = self.document(params)?;
        let Some(analysis) = analysis(document) else {
            return Ok(json!({ "data": [] }));
        };
        let mut symbols = analysis.symbols.iter().collect::<Vec<_>>();
        symbols.sort_by_key(|symbol| symbol.span.start);

        let mut previous_line = 0_u32;
        let mut previous_character = 0_u32;
        let mut data = Vec::with_capacity(symbols.len() * 5);
        for symbol in symbols {
            let start = position(&document.text, symbol.span.start);
            let end = position(&document.text, symbol.span.end);
            if start[0] != end[0] {
                continue;
            }
            let delta_line = start[0] - previous_line;
            let delta_start = if delta_line == 0 {
                start[1] - previous_character
            } else {
                start[1]
            };
            data.extend([
                delta_line,
                delta_start,
                end[1] - start[1],
                if symbol.name == "self" {
                    11
                } else {
                    semantic_token_kind(symbol.kind)
                },
                u32::from(symbol.is_definition),
            ]);
            previous_line = start[0];
            previous_character = start[1];
        }
        Ok(json!({ "data": data }))
    }
}
