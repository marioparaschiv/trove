---
tags: [trabajo, proveedores]
fecha: 2025-04-23
lugar: Valencia, oficinas de Transportes Llorente (Polígono Fuente del Jarro)
---
Reunión de todo el día con los transportistas españoles del piloto. Vuelo KL1503 a las 07:05, de vuelta a las 20:40. Agotador pero muy útil.

## Asistentes

- Rafael Llorente (director, Transportes Llorente, 85 camiones)
- Marta Sanchis (operaciones, Frío Turia, cadena de frío, 32 camiones frigoríficos)
- Iker Goikoetxea (Logística Albufera, el más escéptico)
- Tomás y yo por parte de Kargolo

## Tarifas

Llorente nos avisó de que sube sus tarifas un 9% a partir del 1 de junio por el gasóleo y los peajes nuevos de la AP-7. No nos afecta directamente, pero sus clientes nos lo van a echar en cara si encima el muelle en Berlín les hace esperar. Marta dijo que en Frío Turia cada hora de espera con el motor del frigorífico encendido les cuesta unos 38 €. Ese número lo quiero en la charla de Rotterdam.

Iker se quejó de que pagan 4 portales distintos de reserva de muelles y que no quiere un quinto. Le entiendo perfectamente.

## Integración con Atlas

- Llorente ya usa nuestra API antigua, la versión 1 con polling cada 60 s. Quieren webhooks. Les dije que no antes del cuarto trimestre (ver [[Q3 planning offsite]]).
- Frío Turia necesita poder marcar las cargas refrigeradas para que les den prioridad en la puerta. Buena idea, ticket ATL-187.
- Albufera solo usará la página web, sin integración.
- Problema serio: cuando nuestra API devuelve 429 sus sistemas reintentan al instante. Que lo arreglen ellos, pero tenemos que documentarlo mejor.

## Acuerdos

1. Prueba con Llorente y Frío Turia en Lagerhaus Spree a partir del go-live (ver [[Project Atlas kickoff]]).
2. Sin coste para ellos durante el piloto. Después, 0 € para transportistas, siempre; pagan los almacenes.
3. Llamada de seguimiento cada mes, primer martes, 10:00.

## Tareas

- Tomás: mandar la documentación de la API y el límite de 120 peticiones por minuto
- Yo: preguntarle a Jens si el campo "refrigerado" cabe en la versión 1
- Yo: repasar mi español de correos formales, se me sigue escapando el subjuntivo, ver [[Spanish subjunctive]]

Comparado con el lío de [[Vendor contract renewal ACME]], esto fue un paseo.
