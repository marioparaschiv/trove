---
tags: [testing, rust]
fecha: 2025-02-12
---

## Por qué

Notas para Sofía Ibarra y el equipo de Madrid, que se incorporan al ledger-svc. Los mocks del repositorio nos mintieron dos veces en 2024: el mock aceptaba importes negativos y Postgres no (restricción `amount_minor >= 0`), y el mock ordenaba por id mientras la consulta real ordenaba por fecha. Desde enero de 2025 la regla es: todo lo que toque SQL se prueba contra un Postgres de verdad.

## Montaje

- testcontainers-rs 0.23 con la imagen `postgres:16.4-alpine`.
- Un contenedor por binario de pruebas, no por prueba. Cada prueba crea su propio esquema `t_<ulid>` y lo borra al terminar.
- Arranque del contenedor: 2,8 s en local, 6,1 s en el runner de GitLab.
- Las migraciones se aplican una sola vez con `sqlx migrate` dentro de un `OnceCell`.
- El reloj es un `FixedClock` inyectado, ver [[Dependency injection]].

## Qué cubrimos

- Flujo completo de un pago a proveedor: crear, reservar saldo, enviar a Brightwell (servidor falso con wiremock en el puerto 18080), confirmar.
- Idempotencia: la misma clave dos veces produce un solo pago.
- Concurrencia: 50 tareas de tokio retirando del mismo saldo de 1.000 €; el saldo nunca queda negativo.
- Las dos ramas de cada flag activo ([[Feature flags]]).

## Problemas que nos encontramos

- Puertos: el runner asigna puertos aleatorios. Usar `get_host_port_ipv4(5432)`, nunca el 5432 fijo.
- Sofía descubrió que las pruebas fallaban solo los lunes: un fixture con fecha 2025-02-10 y lógica de "siguiente día hábil". Con el reloj fijo se acabó.
- La suite tarda 4 min 20 s con 212 pruebas. Objetivo: menos de 5 minutos. Lo que sigue fallando de vez en cuando está en [[CI pipeline flakiness]].

## Relacionado

[[Code review checklist]]
