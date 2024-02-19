mqttx conn --hostname localhost --mqtt-version 5 \
  --session-expiry-interval 300 --keepalive 60 --username admin --password public

adb -s emulator-5554 pull /etc/hosts hosts

adb push hosts /etc/hosts

bash ../../cli/cli3.sh mqtt

mqttx conn -h 'localhost' -p 1884 -u 'admin' -P 'public'