#!/bin/bash

# Colors for output
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

echo -e "${BLUE}╔═══════════════════════════════════════╗${NC}"
echo -e "${BLUE}║      FFcut v1.0.0 - Installation      ║${NC}"
echo -e "${BLUE}╚═══════════════════════════════════════╝${NC}"
echo ""

# Check for root privileges
if [[ $EUID -eq 0 ]]; then
   echo -e "${RED}This script should not be run as root${NC}" 
   echo -e "${YELLOW}Please run as normal user${NC}"
   exit 1
fi

# Check FFmpeg
echo -e "${YELLOW}[1/4]${NC} Checking FFmpeg..."
if ! command -v ffmpeg &> /dev/null; then
    echo -e "${YELLOW}FFmpeg not found. Installing...${NC}"
    
    # Detect package manager
    if command -v pacman &> /dev/null; then
        sudo pacman -S --noconfirm ffmpeg
    elif command -v apt &> /dev/null; then
        sudo apt update && sudo apt install -y ffmpeg
    elif command -v dnf &> /dev/null; then
        sudo dnf install -y ffmpeg
    elif command -v zypper &> /dev/null; then
        sudo zypper install -y ffmpeg
    else
        echo -e "${RED}✗ Could not detect package manager${NC}"
        echo -e "${YELLOW}Please install FFmpeg manually${NC}"
        exit 1
    fi
    
    if [ $? -eq 0 ]; then
        echo -e "${GREEN}✓ FFmpeg installed${NC}"
    else
        echo -e "${RED}✗ Failed to install FFmpeg${NC}"
        exit 1
    fi
else
    echo -e "${GREEN}✓ FFmpeg already installed ($(ffmpeg -version | head -n1 | cut -d' ' -f3))${NC}"
fi

# Check Node.js
echo -e "${YELLOW}[2/4]${NC} Checking Node.js..."
if ! command -v node &> /dev/null; then
    echo -e "${YELLOW}Node.js not found. Installing...${NC}"
    
    # Detect package manager
    if command -v pacman &> /dev/null; then
        sudo pacman -S --noconfirm nodejs npm
    elif command -v apt &> /dev/null; then
        sudo apt update && sudo apt install -y nodejs npm
    elif command -v dnf &> /dev/null; then
        sudo dnf install -y nodejs npm
    elif command -v zypper &> /dev/null; then
        sudo zypper install -y nodejs npm
    else
        echo -e "${RED}✗ Could not detect package manager${NC}"
        echo -e "${YELLOW}Please install Node.js manually${NC}"
        exit 1
    fi
    
    if [ $? -eq 0 ]; then
        echo -e "${GREEN}✓ Node.js installed${NC}"
    else
        echo -e "${RED}✗ Failed to install Node.js${NC}"
        exit 1
    fi
else
    NODE_VERSION=$(node --version)
    NPM_VERSION=$(npm --version)
    echo -e "${GREEN}✓ Node.js already installed ($NODE_VERSION)${NC}"
    echo -e "${GREEN}✓ npm installed ($NPM_VERSION)${NC}"
fi

# Install dependencies
echo -e "${YELLOW}[3/4]${NC} Installing npm dependencies..."
npm install
if [ $? -eq 0 ]; then
    echo -e "${GREEN}✓ Dependencies installed${NC}"
else
    echo -e "${RED}✗ Error installing dependencies${NC}"
    exit 1
fi

# Create launch shortcut
echo -e "${YELLOW}[4/4]${NC} Creating launch shortcut..."
cat > start.sh << 'EOF'
#!/bin/bash
cd "$(dirname "$0")"
npm start
EOF

chmod +x start.sh
echo -e "${GREEN}✓ Shortcut created${NC}"

# Create build script
cat > build.sh << 'EOF'
#!/bin/bash
cd "$(dirname "$0")"
npm run build:linux
EOF

chmod +x build.sh
echo -e "${GREEN}✓ Build script created${NC}"

echo ""
echo -e "${GREEN}╔════════════════════════════════════════╗${NC}"
echo -e "${GREEN}║   Installation completed successfully! ║${NC}"
echo -e "${GREEN}╚════════════════════════════════════════╝${NC}"
echo ""
echo -e "To run the application:"
echo -e "${BLUE}  ./start.sh${NC}  or  ${BLUE}npm start${NC}"
echo ""
echo -e "To build the application:"
echo -e "${BLUE}  ./build.sh${NC}  or  ${BLUE}npm run build${NC}"
echo ""
echo -e "Build output will be in: ${BLUE}dist/${NC}"
echo ""
