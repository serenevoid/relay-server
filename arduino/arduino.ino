String str = "";
uint8_t RELAY[] = {2,3,4,5,6,7,8,9,10,11};
uint8_t COUNT = sizeof(RELAY)/sizeof(RELAY[0]);
unsigned long lastTriggeredTime[10] = {0};
unsigned long now = 0;
int TIME_DELAY = 10000;

void setup() {
  Serial.begin(9600);
  for (uint8_t i = 0; i < COUNT; i++) {
    pinMode(RELAY[i], OUTPUT);
    digitalWrite(RELAY[i], HIGH);
  }
}

void loop() {
  if (Serial.available() <= 0) return;
  str = Serial.readString();
  str.trim();
  now = millis();
  uint16_t states = str.toInt();
  for (uint8_t i = 0; i < COUNT; i++) {
    if (now - lastTriggeredTime[i] > TIME_DELAY) {
      if (digitalRead(RELAY[i]) == ((states >> i) & 1)) {
        digitalWrite(RELAY[i], !((states >> i) & 1));
        lastTriggeredTime[i] = now;
      }
    }
  }
  int currentStates = 0;
  for (uint8_t i = 0; i < COUNT; i++) {
    if (digitalRead(RELAY[i]) == LOW) {
      currentStates |= (1 << i);
    }
  }
  Serial.println(currentStates);
}