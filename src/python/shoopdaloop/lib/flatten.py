def flatten(val):
        if type(val) is list:
            r = []
            for elem in val:
                r = r + flatten(elem)
            return r
        return [val]