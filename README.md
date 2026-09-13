# nullscan

Analizador de código estático, rápido y open-source, escrito en Rust. Arranca con soporte
para TypeScript. Nace como alternativa liviana a SonarQube: sin JVM, con CLI de primera
clase, y sin las limitaciones de la edición community (análisis de ramas, etc.).

## Estado actual

PoC funcional: parseo de TypeScript con [tree-sitter](https://tree-sitter.github.io/tree-sitter/)
y un CLI que corre un set de reglas contra un archivo `.ts`.

## Estructura

```
nullscan/
├── nullscan-core/   # parsing (tree-sitter-typescript) + reglas
└── nullscan-cli/    # CLI (clap)
```

## Uso

```sh
cargo run -p nullscan-cli -- archivo.ts
```

Imprime un diagnóstico por línea (`archivo:línea:columna: [regla] mensaje`) y sale con
código 1 si encontró algo, 0 si el archivo está limpio.

## Reglas implementadas

| Regla | Qué detecta |
|---|---|
| `no-console` | Llamadas a `console.*` |
| `no-debugger` | Statement `debugger;` |
| `no-var` | Uso de `var` en vez de `let`/`const` |
| `eqeqeq` | `==`/`!=` en vez de `===`/`!==` |
| `no-explicit-any` | Tipo `any` explícito |
| `no-empty-catch` | Bloque `catch` vacío |

## Desarrollo

```sh
cargo test --workspace
```

Ver [FINDINGS.md](./FINDINGS.md) para gotchas técnicos descubiertos en el camino
(tree-sitter, arquitectura de reglas, etc.).

## Roadmap

- Reglas con estado (complejidad ciclomática, variables no usadas, duplicación de código)
- Output SARIF para integración con CI/editores
- Análisis multi-archivo / glob
- Más lenguajes
