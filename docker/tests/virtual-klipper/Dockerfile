FROM python:3.12-alpine
COPY server.py /srv/server.py
USER 65534:65534
EXPOSE 7125
CMD ["python3", "/srv/server.py"]
