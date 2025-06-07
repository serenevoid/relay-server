String str = "";
uint8_t RELAY[] = {2,3,4,5,6,7,8,9,10,11};
uint8_t COUNT = sizeof(RELAY)/sizeof(RELAY[0]);

void setup() {
  Serial.begin(9600);
  for (uint8_t i = 0; i < COUNT; i++) {
    pinMode(RELAY[i], OUTPUT);
    digitalWrite(RELAY[i], HIGH);
    pinMode(LED_BUILTIN, OUTPUT);
  }
}

void loop() {
  if (Serial.available() <= 0) return;
  str = Serial.readStringUntil('\n');
  str.trim();

  bool isNumeric = true;
  for (unsigned int i = 0; i < str.length(); i++) {
    if (!isDigit(str.charAt(i))) {
      isNumeric = false;
      break;
    }
  }

  if (isNumeric && str.length() > 0) {
    uint16_t states = str.toInt();
    for (uint8_t i = 0; i < COUNT; i++) {
      if (digitalRead(RELAY[i]) == ((states >> i) & 1)) {
        digitalWrite(RELAY[i], !((states >> i) & 1));
      }
    }
    int currentStates = 0;
    for (uint8_t i = 0; i < COUNT; i++) {
      if (digitalRead(RELAY[i]) == LOW) {
        currentStates |= (1 << i);
        digitalWrite(LED_BUILTIN, HIGH);
        delay(50);
        digitalWrite(LED_BUILTIN, LOW);
        delay(50);
      }
    }
    Serial.println(currentStates);
  }
}
