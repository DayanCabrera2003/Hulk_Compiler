# Sección 01 — Setup del proyecto y arquitectura clean

## Resumen
Establece la base del proyecto: workspace de Cargo, crates esqueleto, CI, GitFlow y documentación inicial.

## Posición en el pipeline
Primera sección, no depende de ninguna anterior. Todas las demás dependen de esta.

## Decisiones técnicas

### Decisión 1: Uso de Cargo workspace con resolver 2
- **Qué se eligió**: Workspace de Cargo con `resolver = "2"` y todos los crates como miembros.
- **Alternativas consideradas**:
  - Un solo crate: menos granularidad, difícil de mantener a medida que crece.
  - Crates separados sin workspace: gestión de dependencias y versiones más compleja.
- **Justificación**: El workspace permite compilar, testear y versionar todo el proyecto de forma centralizada, con dependencias compartidas y sin duplicación.

## API expuesta por esta capa
No aplica, solo estructura de proyecto.

## Estrategia de testing
No aplica, aún no hay lógica implementada.

## Lecciones aprendidas y gotchas
Asegurarse de que todos los crates estén listados en `members` y que el resolver esté en `2` para evitar problemas de dependencias transitivas.
