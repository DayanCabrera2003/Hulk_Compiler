# Guía de Contribución — Hulk Compiler

## Flujo de ramas (GitFlow adaptado)

- `main`: rama base, siempre limpia y estable.
- `develop`: rama de integración, parte de `main`.
- `section/X-...`: ramas por sección mayor del proyecto, salen de `develop`.
- `feature/X.Y-...`: ramas de funcionalidad, salen de la rama de sección correspondiente.
- Releases: se fusionan de `develop` a `main`.

## Formato de commits

- `[SNN.M.T] descripción imperativa`
  - Ejemplo: `[S01.2.1] Configura GitFlow con ramas base`

## Checklist pre-merge (auto-impuesto)

- [ ] Todos los tests pasan localmente
- [ ] Documentación relevante actualizada
- [ ] Mensaje de commit con formato correcto
- [ ] `cargo fmt --all --check` sin cambios
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` sin warnings

## Política de merges

- Siempre usar `--no-ff` para merges
- No se usan PRs ni branch protection (proyecto single-developer)
- La disciplina se mantiene mediante este checklist

---

## Notas

- Si se requiere una excepción al flujo, documentar la razón en el commit.
- Comparación con otros flujos (ver doc/seccion-01-setup.md).
