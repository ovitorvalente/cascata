# Roteiro do vídeo — Cascata

Vídeo de apresentação do **Cascata**, aplicativo para GNOME que baixa vídeos
e áudios da internet usando o yt-dlp e o FFmpeg.

## Ficha

| | |
| --- | --- |
| Duração | 60 segundos (há uma versão de 30 s no fim) |
| Formato | 16:9, 1920×1080, 30 fps |
| Idioma | Português do Brasil |
| Tom | Direto e leve; mostra o app funcionando, sem exagero |
| Público | Quem usa Linux com GNOME (Fedora, Ubuntu) |
| Trilha | Instrumental calma, com batida suave |

## Identidade visual

- **Ícone:** `icone.svg` (quadrado azul arredondado, três quedas d'água e uma
  seta de download).
- **Cores do app:**
  - azul do botão principal `#3584e4`
  - azul da série de vídeo `#2a78d6` (tema escuro: `#3987e5`)
  - laranja da série de áudio `#eb6834` (tema escuro: `#d95926`)
  - verde de concluído `#0ca30c`, vermelho de erro `#d03b3b`
  - fundo claro `#fafafb`, fundo escuro `#222226`
- **Fonte:** Adwaita Sans (a do GNOME); Inter serve como alternativa.
- **Fundo das cenas:** degradê suave de azul-claro para branco no tema claro;
  na cena do tema escuro, azul-marinho para quase preto.

## Como usar os prints

Os prints estão em `prints/`, em escala 2× (nítidos para zoom). São janelas
em pé, mais altas que largas: no quadro 16:9, coloque a janela de um lado e
o texto do outro, ou duas janelas lado a lado.

As janelas foram fotografadas com cantos retos e sem sombra. Na edição,
aplique **cantos arredondados (cerca de 24 px nos prints 2×) e uma sombra
suave** para ficarem como no GNOME.

## Cenas (60 s)

### Cena 1 — Abertura (0:00–0:05)

- **Imagem:** `icone.svg` grande no centro, sobre o degradê claro.
- **Movimento:** o ícone surge com um leve salto; as três quedas d'água
  "escorrem" de cima para baixo e a seta desce até a linha de base. O nome
  aparece em seguida.
- **Texto na tela:** **Cascata** · *Baixe vídeos e áudios, direto do GNOME*
- **Narração:** "Este é o Cascata: um jeito simples de baixar vídeos e
  áudios no Linux."

### Cena 2 — Cole o endereço (0:05–0:12)

- **Imagens:** `01-janela-principal.png` → `02-endereco-colado.png`
- **Movimento:** a janela entra pela direita. O endereço é digitado no campo
  "Endereço do vídeo" (transição do print 01 para o 02). Destaque rápido em
  "Formato" e "Qualidade". O cursor clica em **Baixar**.
- **Texto na tela:** *Cole o endereço. Escolha o formato. Baixe.*
- **Narração:** "Cole o endereço, escolha vídeo ou só o áudio, a qualidade,
  e clique em Baixar."

### Cena 3 — Download em andamento (0:12–0:22)

- **Imagens:** `03-baixando-video.png` → `06-baixando-audio.png` →
  `07-processando.png` → `08-concluido.png`
- **Movimento:** a barra de progresso avança e o gráfico de velocidade se
  desenha da esquerda para a direita, primeiro em azul (vídeo), depois em
  laranja (áudio). Zoom na linha de estado: "Baixando — 26% · 47,6 MB de
  183,0 MB · 12,5 MB/s · restam 0:08". No fim, o aviso "Download concluído"
  sobe na base da janela.
- **Texto na tela:** *Progresso, velocidade e tempo restante, ao vivo*
- **Narração:** "Você acompanha tudo em tempo real: quanto já baixou, a
  velocidade e quanto falta. Quando termina, é só abrir a pasta."

### Cena 4 — Painel de detalhes (0:22–0:32)

- **Imagens:** `04-detalhes-video.png` → `05-detalhes-audio.png` →
  `09-detalhes-concluido.png`
- **Movimento:** o painel "Detalhes do download" desliza para o lado da
  janela principal. O anel de progresso enche até 89%. Zoom no gráfico, com
  a legenda Vídeo/Áudio em destaque. Depois, rolagem curta pelos números:
  velocidade média, velocidade máxima e tempo decorrido.
- **Texto na tela:** *Todos os detalhes em um painel*
- **Narração:** "Quer ver mais? O painel de detalhes mostra a miniatura, o
  gráfico de velocidade do vídeo e do áudio, e todos os números."

### Cena 5 — Segundo plano e bandeja (0:32–0:41)

- **Imagens:** `20-bandeja-icones.png`, `10-preferencias.png`,
  `11-bandeja-extensao.png`
- **Movimento:** a janela principal se fecha e "encolhe" até virar o ícone
  da bandeja, no canto superior da tela. O ícone anima de 7 → 62 → 99 e
  termina no "check" verde (use os estados do print 20). Em seguida, corte
  para as preferências com zoom no grupo "Segundo plano", e o diálogo
  "Instalar a extensão de bandeja?" aparecendo por cima.
- **Texto na tela:** *Feche a janela. O download continua.*
- **Narração:** "Pode fechar a janela: o download continua em segundo plano
  e o ícone na bandeja mostra a porcentagem. Se o GNOME precisar de uma
  extensão para isso, o Cascata instala para você."

### Cena 6 — Instala o que falta (0:41–0:47)

- **Imagens:** `13-dependencia-ausente.png` → `14-instalar-dependencias.png`
- **Movimento:** a faixa "yt-dlp não está instalado" desce no topo da
  janela; o cursor clica em **Instalar** e o diálogo com o comando aparece.
- **Texto na tela:** *Faltou alguma ferramenta? Ele instala.*
- **Narração:** "Na primeira vez, se faltar o yt-dlp ou o FFmpeg, o Cascata
  avisa e instala, no Fedora e no Ubuntu."

### Cena 7 — Do seu jeito (0:47–0:53)

- **Imagens:** `03-baixando-video.png` e `18-escuro-baixando.png` lado a
  lado; depois `19-ingles.png` e `17-escuro-detalhes.png`
- **Movimento:** tela dividida na diagonal, claro de um lado e escuro do
  outro; uma "cortina" passa trocando o tema. Depois, os textos da janela
  trocam de português para inglês.
- **Texto na tela:** *Tema claro ou escuro · Português ou inglês*
- **Narração:** "Ele acompanha o tema do seu sistema e fala português e
  inglês."

### Cena 8 — Encerramento (0:53–1:00)

- **Imagem:** `icone.svg` no centro, sobre o degradê claro;
  `12-sobre.png` pode aparecer pequeno ao fundo, desfocado.
- **Movimento:** o ícone volta ao centro; o nome e a linha de apoio
  aparecem um depois do outro.
- **Texto na tela:** **Cascata** · *Código aberto · Feito com Rust, GTK 4 e
  libadwaita* · `[endereço do projeto]`
- **Texto pequeno no rodapé:** *Baixe apenas conteúdo próprio ou com
  permissão do autor.*
- **Narração:** "Cascata. Gratuito, de código aberto, feito para o GNOME."

## Versão curta (30 s)

1. **0:00–0:04** — Cena 1 (abertura).
2. **0:04–0:10** — Cena 2 (colar o endereço e clicar em Baixar).
3. **0:10–0:18** — Cena 3 (download com o gráfico).
4. **0:18–0:24** — Cena 5, só a parte do ícone na bandeja com a porcentagem.
5. **0:24–0:30** — Cena 8 (encerramento).

Narração da versão curta: "Cascata: cole o endereço, clique em Baixar e
acompanhe tudo em tempo real. Feche a janela e o download continua, com a
porcentagem na bandeja. Gratuito e de código aberto, para o GNOME."

## Texto pronto para colar na ferramenta de vídeo

```
Crie um vídeo de apresentação de 60 segundos, 16:9 (1920×1080), em
português do Brasil, para o aplicativo "Cascata": um app para GNOME (Linux)
que baixa vídeos e áudios da internet.

Estilo: motion graphics limpo, no visual do GNOME/libadwaita. Fundo em
degradê suave de azul-claro para branco. Cores de destaque: azul #3584e4,
laranja #eb6834 e verde #0ca30c. Fonte Adwaita Sans ou Inter. Transições
suaves (deslizar, zoom leve), sem efeitos chamativos. Trilha instrumental
calma.

Use as imagens anexas (prints reais do app, em escala 2×). Elas são janelas
em pé: posicione a janela de um lado do quadro e o texto do outro. Aplique
cantos arredondados e sombra suave em cada janela.

Cenas:
1. (0–5 s) icone.svg no centro, surgindo com um leve salto. Texto:
   "Cascata — Baixe vídeos e áudios, direto do GNOME".
2. (5–12 s) 01-janela-principal → 02-endereco-colado: o endereço é digitado
   e o cursor clica em "Baixar". Texto: "Cole o endereço. Escolha o
   formato. Baixe."
3. (12–22 s) 03-baixando-video → 06-baixando-audio → 07-processando →
   08-concluido: a barra avança e o gráfico se desenha, azul e depois
   laranja. Texto: "Progresso, velocidade e tempo restante, ao vivo".
4. (22–32 s) 04-detalhes-video → 05-detalhes-audio → 09-detalhes-concluido:
   o painel desliza para o lado da janela, o anel enche, zoom no gráfico.
   Texto: "Todos os detalhes em um painel".
5. (32–41 s) A janela encolhe até virar o ícone da bandeja, que anima de 7
   para 62, 99 e um "check" verde (estados em 20-bandeja-icones). Depois
   10-preferencias com zoom em "Segundo plano" e 11-bandeja-extensao.
   Texto: "Feche a janela. O download continua."
6. (41–47 s) 13-dependencia-ausente → 14-instalar-dependencias. Texto:
   "Faltou alguma ferramenta? Ele instala."
7. (47–53 s) Tela dividida: 03-baixando-video (claro) e 18-escuro-baixando
   (escuro); depois 19-ingles. Texto: "Tema claro ou escuro · Português ou
   inglês".
8. (53–60 s) icone.svg no centro. Texto: "Cascata — Código aberto · Feito
   com Rust, GTK 4 e libadwaita". Rodapé pequeno: "Baixe apenas conteúdo
   próprio ou com permissão do autor."

Narração (voz calma, em português do Brasil):
"Este é o Cascata: um jeito simples de baixar vídeos e áudios no Linux.
Cole o endereço, escolha vídeo ou só o áudio, a qualidade, e clique em
Baixar. Você acompanha tudo em tempo real: quanto já baixou, a velocidade e
quanto falta. Quer ver mais? O painel de detalhes mostra o gráfico de
velocidade e todos os números. Pode fechar a janela: o download continua em
segundo plano e o ícone na bandeja mostra a porcentagem. Se faltar alguma
ferramenta, o Cascata avisa e instala. Ele acompanha o tema do seu sistema
e fala português e inglês. Cascata. Gratuito, de código aberto, feito para
o GNOME."
```

## Lista dos prints

| Arquivo | O que mostra |
| --- | --- |
| `01-janela-principal.png` | Janela principal, vazia |
| `02-endereco-colado.png` | Endereço preenchido, antes de baixar |
| `03-baixando-video.png` | Baixando o vídeo: 26%, gráfico em azul, botão Cancelar |
| `04-detalhes-video.png` | Painel de detalhes durante o vídeo |
| `05-detalhes-audio.png` | Painel de detalhes durante o áudio: anel em 89%, gráfico em duas cores |
| `06-baixando-audio.png` | Janela principal durante o áudio |
| `07-processando.png` | Etapa do FFmpeg ("Processando…") |
| `08-concluido.png` | Concluído, com o aviso "Mostrar na pasta" |
| `09-detalhes-concluido.png` | Painel de detalhes depois de concluir |
| `10-preferencias.png` | Preferências: idioma, downloads e segundo plano |
| `11-bandeja-extensao.png` | Diálogo "Instalar a extensão de bandeja?" |
| `12-sobre.png` | Diálogo "Sobre o Cascata" |
| `13-dependencia-ausente.png` | Faixa "yt-dlp não está instalado" |
| `14-instalar-dependencias.png` | Diálogo com o comando de instalação |
| `15-erro.png` | Diálogo "O download falhou" |
| `16-escuro-janela.png` | Janela principal no tema escuro |
| `17-escuro-detalhes.png` | Painel de detalhes no tema escuro |
| `18-escuro-baixando.png` | Download em andamento no tema escuro |
| `19-ingles.png` | Download em andamento com a interface em inglês |
| `20-bandeja-icones.png` | Estados do ícone da bandeja, ampliados e em tamanho real |

## Observações sobre os prints

- **São do app real, com um download simulado.** As telas foram fotografadas
  do próprio Cascata rodando em uma tela virtual. Para não depender da rede
  nem usar conteúdo de terceiros, o download é simulado: o título
  "Cachoeiras do Brasil — documentário (4K)", a miniatura e os endereços
  `exemplo.com.br` são fictícios, criados para as capturas.
- **A bandeja não foi fotografada.** O menu e o ícone na barra do GNOME são
  desenhados pelo sistema e não aparecem na tela virtual. O print 20 é o
  desenho exato que o app envia para a bandeja. Para mostrar o menu de
  verdade, tire um print na sua máquina (tecla Print Screen) com um
  download em andamento e o menu da bandeja aberto.
- **Faltou uma tela:** a página "Solução de problemas" do diálogo "Sobre"
  não foi capturada.
- **Endereço do projeto:** troque `[endereço do projeto]` pelo link quando o
  repositório for público.
