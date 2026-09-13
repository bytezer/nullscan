# Findings

Notas técnicas y gotchas descubiertos durante el desarrollo, para no repetir la
investigación (o el error) la próxima vez.

## tree-sitter: los predicados `#eq?`/`#match?` no se evalúan solos

`QueryCursor::matches` (crate `tree-sitter` 0.27, binding de Rust) **no** filtra las
coincidencias usando predicados de texto como `(#eq? @cap "algo")`. La query devuelve
todos los matches estructurales sin importar el predicado — hay que leer el texto del
nodo capturado (`node.utf8_text(source.as_bytes())`) y filtrar a mano en Rust.

**Cómo se manifestó:** la regla `no-explicit-any` usaba
`(predefined_type) @report (#eq? @report "any")` y marcaba *todos* los tipos primitivos
(`number`, `boolean`, `string`...), no solo `any`. Un test (`allows_concrete_types`) lo
agarró de inmediato.

**Regla práctica:** usar queries de tree-sitter solo para condiciones *estructurales*
(forma del árbol, tokens literales como `"var"` o `"=="` dentro del patrón). Cualquier
condición sobre el *contenido/texto* de un nodo capturado se filtra en Rust después de
`cursor.matches(...)`, no en el predicado de la query.

Ver `no_console_rule` y `no_explicit_any_rule` en `nullscan-core/src/lib.rs` para el
patrón a seguir.
