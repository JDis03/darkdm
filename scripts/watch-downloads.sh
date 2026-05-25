#!/bin/bash
DIR="$HOME/Descargas/DarkDM"
mkdir -p "$DIR"

while true; do
  clear
  echo -e "\033[1;36m━━━ DarkDM — Monitor de Descargas ━━━\033[0m"
  echo -e "\033[2mCtrl+C para salir | Actualiza cada 3s\033[0m\n"
  echo -e "\033[1;33m📁 $DIR\033[0m\n"

  files=()
  while IFS= read -r f; do
    files+=("$f")
  done < <(find "$DIR" -maxdepth 1 -type f -size +0 2>/dev/null | sort -r)

  if [ ${#files[@]} -eq 0 ]; then
    echo "   No hay descargas aún"
    echo "   Haz clic en ⬇️ DarkDM en cualquier video"
  else
    total=0
    for f in "${files[@]}"; do
      size=$(stat --printf="%s" "$f" 2>/dev/null || echo 0)
      total=$((total + size))
      name=$(basename "$f" | cut -c1-65)
      
      if [ "$size" -ge 1048576 ]; then
        sz=$((size / 1048576))"MB"
      elif [ "$size" -ge 1024 ]; then
        sz=$((size / 1024))"KB"
      else
        sz="${size}B"
      fi
      
      modified=$(stat --printf="%Y" "$f" 2>/dev/null || echo 0)
      diff=$(( $(date +%s) - modified ))
      if [ "$diff" -lt 60 ]; then tm="ahora"
      elif [ "$diff" -lt 3600 ]; then tm="$((diff / 60))m"
      elif [ "$diff" -lt 86400 ]; then tm="$((diff / 3600))h"
      else tm="$((diff / 86400))d"
      fi
      
      icon="📄"
      case "${f,,}" in *.mp4|*.webm|*.mkv) icon="🎬";; *.ts) icon="📺";; *.mp3) icon="🎵";; esac
      
      echo -e "   ${icon} \033[1;37m${name}\033[0m"
      echo -e "      \033[2m${sz} • ${tm} ago\033[0m"
    done
    
    if [ "$total" -ge 1048576 ]; then
      ts=$((total / 1048576))" MB"
    elif [ "$total" -ge 1024 ]; then
      ts=$((total / 1024))" KB"
    else
      ts="${total} B"
    fi
    echo ""
    echo -e "   \033[1;36mTotal: ${ts} | ${#files[@]} archivos\033[0m"
  fi
  
  sleep 3
done
