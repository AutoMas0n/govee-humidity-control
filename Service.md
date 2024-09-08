1. **Create a Service File**:
   ```sh
   sudo nano /etc/systemd/system/myscript.service
   ```

2. **Add the Following Content**:
```ini
[Unit]
Description=My Script Service
After=network.target

[Service]
ExecStart=/usr/bin/python3 /home/pi/Github/govee-humidity-control/main.py
WorkingDirectory=/home/pi/Github/govee-humidity-control
StandardOutput=inherit
StandardError=inherit
Restart=always
User=pi

[Install]
WantedBy=multi-user.target
```

   Replace `/path/to/your/main.py` and `/path/to/your/script` with the actual paths to your script.

3. **Reload Systemd and Enable the Service**:
   ```sh
   sudo systemctl daemon-reload
   sudo systemctl enable myscript.service
   sudo systemctl start myscript.service
   ```