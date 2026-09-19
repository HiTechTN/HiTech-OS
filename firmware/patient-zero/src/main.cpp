/*
 * HiTech-OS — Firmware "Patient Zero" (ESP32)
 *
 * Rôle : lecture DHT22 + BMS JK (UART) + affichage OLED, publication MQTT,
 * pilotage du relais de refroidissement (seuil 30°C).
 *
 * Mode survie (Section 1.3 de la roadmap) :
 *   - Le noeud est alimenté en permanence par la station 24V => pas besoin
 *     de persistance survivant a une coupure d'alimentation.
 *   - Tampon de telemetrie en RTC RAM (survit aux soft reboots) plutot
 *     qu'en Flash, pour eviter l'usure prematuree de la puce en cas de
 *     coupure reseau longue.
 *   - Les seuils critiques (relais de refroidissement a 30 C) restent
 *     appliques localement, independamment de l'etat du broker MQTT.
 *
 * Identifiants WiFi/MQTT : voir secrets.h (non versionne, voir
 * secrets.h.example).
 */

#include <Arduino.h>
#include <WiFi.h>
#include <PubSubClient.h>
#include <DHT.h>
#include <Wire.h>
#include <Adafruit_GFX.h>
#include <Adafruit_SSD1306.h>
#include "secrets.h"

// --- Broches ---
#define DHT_PIN 4
#define DHT_TYPE DHT22
#define RELAY_PIN 5
#define BMS_RX_PIN 16
#define BMS_TX_PIN 17
#define OLED_WIDTH 128
#define OLED_HEIGHT 64

#define COOLING_THRESHOLD_C 30.0f
#define RING_BUFFER_SIZE 64          // en RTC RAM, pas en Flash
#define RECONNECT_MAX_BACKOFF_MS 60000

DHT dht(DHT_PIN, DHT_TYPE);
Adafruit_SSD1306 display(OLED_WIDTH, OLED_HEIGHT, &Wire, -1);
HardwareSerial bmsSerial(2); // UART2 pour le BMS JK
WiFiClient wifiClient;
PubSubClient mqtt(wifiClient);

const char *NODE_ID = "patient-zero-01";
const char *TOPIC_TEMP = "hitechos/lab/patient-zero-01/temperature";
const char *TOPIC_BMS = "hitechos/lab/patient-zero-01/bms";

// --- Tampon de telemetrie en RTC RAM (survit aux soft reboots) ---
RTC_DATA_ATTR struct {
  float temperature[RING_BUFFER_SIZE];
  uint32_t timestamp[RING_BUFFER_SIZE];
  uint8_t head = 0;
  uint8_t count = 0;
} telemetry_buffer;

uint32_t reconnect_backoff_ms = 1000;
uint32_t last_reconnect_attempt = 0;

// --- Regles de survie locales : toujours appliquees, meme sans reseau ---
void applyLocalSurvivalRules(float temperature) {
  bool cooling_needed = temperature > COOLING_THRESHOLD_C;
  digitalWrite(RELAY_PIN, cooling_needed ? HIGH : LOW);
}

void bufferReadingInRam(float temperature) {
  telemetry_buffer.temperature[telemetry_buffer.head] = temperature;
  telemetry_buffer.timestamp[telemetry_buffer.head] = millis();
  telemetry_buffer.head = (telemetry_buffer.head + 1) % RING_BUFFER_SIZE;
  if (telemetry_buffer.count < RING_BUFFER_SIZE) telemetry_buffer.count++;
}

void publishTemperature(float temperature) {
  char payload[64];
  snprintf(payload, sizeof(payload),
           "{\"node\":\"%s\",\"temp_c\":%.2f}", NODE_ID, temperature);
  mqtt.publish(TOPIC_TEMP, payload);
}

// Rejoue le tampon RAM vers MQTT puis le vide (appelee a la reconnexion).
void flushBufferToMqtt() {
  while (telemetry_buffer.count > 0) {
    uint8_t idx = (telemetry_buffer.head + RING_BUFFER_SIZE - telemetry_buffer.count) % RING_BUFFER_SIZE;
    char payload[80];
    snprintf(payload, sizeof(payload),
             "{\"node\":\"%s\",\"temp_c\":%.2f,\"ts_ms\":%lu,\"buffered\":true}",
             NODE_ID, telemetry_buffer.temperature[idx], telemetry_buffer.timestamp[idx]);
    mqtt.publish(TOPIC_TEMP, payload);
    telemetry_buffer.count--;
  }
}

// Lecture non bloquante d'une trame texte simple du BMS JK sur UART.
// Le protocole binaire complet du JK BMS reste a implementer (RFC ouvert) ;
// ceci publie la ligne brute recue en attendant.
void pollBmsUart() {
  if (bmsSerial.available()) {
    String line = bmsSerial.readStringUntil('\n');
    if (line.length() > 0) {
      char payload[160];
      snprintf(payload, sizeof(payload), "{\"node\":\"%s\",\"raw\":\"%s\"}",
               NODE_ID, line.c_str());
      mqtt.publish(TOPIC_BMS, payload);
    }
  }
}

void updateDisplay(float temperature, bool mqtt_ok) {
  display.clearDisplay();
  display.setTextSize(1);
  display.setTextColor(SSD1306_WHITE);
  display.setCursor(0, 0);
  display.printf("HiTech-OS %s\n", NODE_ID);
  display.printf("Temp: %.1f C\n", temperature);
  display.printf("MQTT: %s\n", mqtt_ok ? "OK" : "hors-ligne");
  display.printf("Buffer: %d/%d\n", telemetry_buffer.count, RING_BUFFER_SIZE);
  display.display();
}

// Reconnexion WiFi/MQTT en backoff exponentiel (plafonne).
void tryReconnect() {
  uint32_t now = millis();
  if (now - last_reconnect_attempt < reconnect_backoff_ms) return;
  last_reconnect_attempt = now;

  if (WiFi.status() != WL_CONNECTED) {
    WiFi.begin(WIFI_SSID, WIFI_PASSWORD);
  }
  if (WiFi.status() == WL_CONNECTED && !mqtt.connected()) {
    if (mqtt.connect(NODE_ID, MQTT_USER, MQTT_PASSWORD)) {
      reconnect_backoff_ms = 1000; // succes -> on repart bas
      flushBufferToMqtt();
      return;
    }
  }
  reconnect_backoff_ms = min(reconnect_backoff_ms * 2, (uint32_t)RECONNECT_MAX_BACKOFF_MS);
}

void setup() {
  Serial.begin(115200);
  bmsSerial.begin(115200, SERIAL_8N1, BMS_RX_PIN, BMS_TX_PIN);

  pinMode(RELAY_PIN, OUTPUT);
  digitalWrite(RELAY_PIN, LOW);

  dht.begin();

  Wire.begin();
  display.begin(SSD1306_SWITCHCAPVCC, 0x3C);

  mqtt.setServer(MQTT_BROKER_HOST, MQTT_BROKER_PORT);
  WiFi.begin(WIFI_SSID, WIFI_PASSWORD);
}

void loop() {
  float temperature = dht.readTemperature();
  bool reading_valid = !isnan(temperature);

  if (reading_valid) {
    applyLocalSurvivalRules(temperature);
  }

  bool mqtt_ok = mqtt.connected();
  if (mqtt_ok) {
    mqtt.loop();
    pollBmsUart();
    if (reading_valid) publishTemperature(temperature);
  } else {
    tryReconnect();
    if (reading_valid) bufferReadingInRam(temperature);
  }

  updateDisplay(reading_valid ? temperature : -99.0f, mqtt_ok);
  delay(1000);
}
