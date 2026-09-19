/*
 * HiTech-OS — Firmware "Patient Zero" (ESP32)
 *
 * Rôle : lecture DHT22 + BMS (UART) + affichage OLED, publication MQTT,
 * pilotage du relais de refroidissement (seuil 30°C).
 *
 * Mode survie (Section 1.3 de la roadmap) :
 *   - Le nœud est alimenté en permanence par la station 24V => pas besoin
 *     de persistance survivant à une coupure d'alimentation.
 *   - Tampon de télémétrie en RTC RAM (survit aux soft reboots) plutôt
 *     qu'en Flash, pour éviter l'usure prématurée de la puce (cycles
 *     d'écriture limités) en cas de coupure réseau longue.
 *   - Les seuils critiques (ex. relais de refroidissement à 30°C) restent
 *     appliqués localement, indépendamment de l'état du broker MQTT.
 */

#include <Arduino.h>

#define COOLING_THRESHOLD_C 30.0f
#define RING_BUFFER_SIZE 64  // en RTC RAM, pas en Flash

// --- Tampon de télémétrie en RTC RAM (survit aux soft reboots) ---
RTC_DATA_ATTR struct {
  float temperature[RING_BUFFER_SIZE];
  uint32_t timestamp[RING_BUFFER_SIZE];
  uint8_t head = 0;
  uint8_t count = 0;
} telemetry_buffer;

bool mqtt_connected = false;

void applyLocalSurvivalRules(float temperature) {
  // Le seuil de sécurité s'applique toujours, même sans réseau/broker.
  bool cooling_needed = temperature > COOLING_THRESHOLD_C;
  // TODO Phase 3 : digitalWrite(RELAY_PIN, cooling_needed);
}

void bufferReadingInRam(float temperature) {
  telemetry_buffer.temperature[telemetry_buffer.head] = temperature;
  telemetry_buffer.timestamp[telemetry_buffer.head] = millis();
  telemetry_buffer.head = (telemetry_buffer.head + 1) % RING_BUFFER_SIZE;
  if (telemetry_buffer.count < RING_BUFFER_SIZE) telemetry_buffer.count++;
}

void flushBufferToMqttOnReconnect() {
  // TODO Phase 3 : rejouer telemetry_buffer vers Mosquitto, puis le vider.
}

void setup() {
  Serial.begin(115200);
  // TODO Phase 3 : init DHT22, OLED, UART BMS, WiFi, MQTT (PubSubClient)
}

void loop() {
  float temperature = 0.0f;  // TODO : lecture réelle DHT22

  applyLocalSurvivalRules(temperature);

  if (mqtt_connected) {
    // publication directe + purge du buffer si des données étaient en attente
    flushBufferToMqttOnReconnect();
  } else {
    bufferReadingInRam(temperature);
    // TODO : reconnexion en backoff exponentiel
  }

  delay(1000);
}
