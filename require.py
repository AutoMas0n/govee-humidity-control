required_imports = ['os', 'requests', 'time', 'logging']
missing_imports = []

for imp in required_imports:
    try:
        __import__(imp)
    except ImportError:
        missing_imports.append(imp)

if missing_imports:
    print(f"Missing imports: {', ',join(missing_imports)}")
else:
    print("All required imports are present.")