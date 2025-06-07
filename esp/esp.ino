#include <ESP8266WiFi.h>
#include <ESP8266WebServer.h>
#include <ESP8266HTTPClient.h>

#ifndef STASSID
#define STASSID "user"
#define STAPSK "pass"
#endif

const char* ssid = STASSID;
const char* password = STAPSK;

unsigned long lastTriggeredTime[10] = {0};
unsigned long now = 0;
uint16_t localStates = 0;
int TIME_DELAY = 1000;
int COUNT = 10;
const int led = LED_BUILTIN;

ESP8266WebServer server(80);

const String postLanding = "<html>\
  <head>\
    <title>ESP8266 Relay Server</title>\
  </head>\
  <body>\
    <h1>ESP Relay Server</h1><br>\
    <p>To change states, send a post request to http://{x.x.x.x}/set as \"text/plain\" and value as number which represents the states in binary form.</p>\
    <p>Number 7 is interpreted as 0000000111 which says that relays 1, 2 and 3 are active.</p>\
  </body>\
</html>";

void registerBoard() {
  WiFiClient client;
  HTTPClient http;
  http.begin(client, "http://10.8.32.142:3402/register");
  http.addHeader("Content-Type", "application/json");

  String payload = "{\"device\":\"relayBoard\",\"ip\":\"" + WiFi.localIP().toString() + "\"}";
  int httpResponseCode = http.POST(payload);

  if (httpResponseCode == 200) {
    Serial.println("Registered board!");
  } else {
    Serial.printf("Registration failed");
  }
  http.end();
}

void handleRoot() {
  digitalWrite(led, 1);
  server.send(200, "text/html", postLanding);
  digitalWrite(led, 0);
}

void handleIP() {
  server.send(200, "text/plain", WiFi.localIP().toString().c_str());
}

void handlePlain() {
  if (server.method() != HTTP_POST) {
    digitalWrite(led, 1);
    server.send(405, "text/plain", "Method Not Allowed");
    digitalWrite(led, 0);
  } else {
    digitalWrite(led, 1);
    now = millis();
    String body = server.arg("plain");
    body.trim();
    if (body.length() == 0 || !body.equals(String(body.toInt()))) {
      server.send(400, "text/plain", "Invalid input");
      return;
    }
    uint16_t states = body.toInt();
    for (uint8_t i = 0; i < COUNT; i++) {
      if (now - lastTriggeredTime[i] > TIME_DELAY) {
        if (((localStates >> i) & 1) != ((states >> i) & 1)) {
          localStates ^= (1 << i);
          lastTriggeredTime[i] = now;
        }
      }
    }
    Serial.println(localStates);
    server.send(200, "text/plain", String(localStates));
    digitalWrite(led, 0);
  }
}

void handleNotFound() {
  digitalWrite(led, 1);
  String message = "File Not Found\n\n";
  message += "URI: ";
  message += server.uri();
  message += "\nMethod: ";
  message += (server.method() == HTTP_GET) ? "GET" : "POST";
  message += "\nArguments: ";
  message += server.args();
  message += "\n";
  for (uint8_t i = 0; i < server.args(); i++) { message += " " + server.argName(i) + ": " + server.arg(i) + "\n"; }
  server.send(404, "text/plain", message);
  digitalWrite(led, 0);
}

void setup(void) {
  pinMode(led, OUTPUT);
  digitalWrite(led, HIGH);
  Serial.begin(9600);
  WiFi.begin(ssid, password);

  // Wait for connection
  while (WiFi.status() != WL_CONNECTED) {
    delay(500);
  }
  digitalWrite(led, LOW);
  registerBoard();
  server.on("/", handleRoot);
  server.on("/set", handlePlain);
  server.on("/esp", handleIP);
  server.onNotFound(handleNotFound);
  server.begin();
  Serial.println("0");
}

void loop(void) {
  server.handleClient();
}
