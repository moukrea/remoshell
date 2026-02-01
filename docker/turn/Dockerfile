# TURN Server Dockerfile
# coturn TURN/STUN server for WebRTC NAT traversal

FROM instrumentisto/coturn:latest

# Expose STUN/TURN ports
# 3478 - STUN/TURN UDP and TCP
# 5349 - STUN/TURN over TLS
# 49152-49200 - Relay ports for media
EXPOSE 3478/udp
EXPOSE 3478/tcp
EXPOSE 5349/udp
EXPOSE 5349/tcp
EXPOSE 49152-49200/udp

# Use turnserver as entrypoint (allows passing config via CMD or args)
ENTRYPOINT ["turnserver"]
