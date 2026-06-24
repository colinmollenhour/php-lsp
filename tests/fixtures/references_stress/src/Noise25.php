<?php

namespace App;

class Noise25
{
    private function compute(): int
    {
        return 25;
    }

    public function process(): int
    {
        return $this->compute() + $this->process();
    }
}
